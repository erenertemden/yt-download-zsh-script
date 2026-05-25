use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{
    config::{
        cycle_index, default_output_dir, default_output_dir_input, expand_output_dir, FORMATS,
        MAX_LOG_LINES, RESOLUTIONS,
    },
    job::{run_download_job, JobConfig},
    media::{
        load_available_formats_with_control, looks_like_playlist_only_url,
        selected_format_container_error,
    },
    process_control::ProcessControl,
    system::open_output_dir,
    types::{AvailableFormat, EncoderMode, Focus, PlaylistProgress, Progress, Screen, WorkerEvent},
};

pub struct App {
    pub screen: Screen,
    pub focus: Focus,
    pub url: String,
    pub resolution_idx: usize,
    pub format_idx: usize,
    pub source_format_idx: usize,
    pub available_formats: Vec<AvailableFormat>,
    pub formats_loading: bool,
    pub convert: bool,
    pub encoder_mode: EncoderMode,
    pub output_dir_input: String,
    pub output_dir: PathBuf,
    pub playlist_progress: Option<PlaylistProgress>,
    pub logs: VecDeque<String>,
    pub progress: Option<Progress>,
    pub status: String,
    pub result_success: Option<bool>,
    worker_rx: Option<Receiver<WorkerEvent>>,
    worker_control: Option<ProcessControl>,
    format_rx: Option<Receiver<Result<Vec<AvailableFormat>, String>>>,
    format_control: Option<ProcessControl>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            screen: Screen::Form,
            focus: Focus::Url,
            url: String::new(),
            resolution_idx: 0,
            format_idx: 0,
            source_format_idx: 0,
            available_formats: Vec::new(),
            formats_loading: false,
            convert: true,
            encoder_mode: EncoderMode::default(),
            output_dir_input: default_output_dir_input(),
            output_dir: default_output_dir(),
            playlist_progress: None,
            logs: VecDeque::new(),
            progress: None,
            status: "Paste a video or playlist URL.".to_string(),
            result_success: None,
            worker_rx: None,
            worker_control: None,
            format_rx: None,
            format_control: None,
        }
    }
}

impl App {
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            if self.screen == Screen::Running {
                self.request_cancel();
                return false;
            }
            return true;
        }

        match self.screen {
            Screen::Form => self.handle_form_key(key),
            Screen::Running => self.handle_running_key(key),
            Screen::Done => self.handle_done_key(key),
        }
    }

    pub fn drain_worker_events(&mut self) {
        self.drain_format_events();

        let mut events = Vec::new();
        let mut disconnected = false;

        if let Some(rx) = self.worker_rx.as_ref() {
            loop {
                match rx.try_recv() {
                    Ok(event) => events.push(event),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }

        for event in events {
            match event {
                WorkerEvent::Log(line) => self.push_log(line),
                WorkerEvent::Progress(progress) => self.progress = Some(progress),
                WorkerEvent::Playlist(progress) => self.playlist_progress = Some(progress),
                WorkerEvent::Done { success, message } => {
                    self.result_success = Some(success);
                    self.status = message.clone();
                    self.push_log(message);
                    self.screen = Screen::Done;
                    self.worker_rx = None;
                    self.worker_control = None;
                }
            }
        }

        if disconnected && self.screen == Screen::Running {
            self.result_success = Some(false);
            self.status = "Worker stopped unexpectedly.".to_string();
            self.push_log("Worker stopped unexpectedly.");
            self.screen = Screen::Done;
            self.worker_rx = None;
            self.worker_control = None;
        }
    }

    pub fn source_format_label(&self) -> &str {
        if self.formats_loading {
            return "loading available formats...";
        }

        if self.source_format_idx == 0 {
            if self.available_formats.is_empty() {
                "Auto best (press f to load)"
            } else {
                "Auto best"
            }
        } else {
            self.available_formats
                .get(self.source_format_idx - 1)
                .map(|format| format.label.as_str())
                .unwrap_or("Auto best")
        }
    }

    pub fn resolution_label(&self) -> &str {
        if self.source_format_idx == 0 {
            RESOLUTIONS[self.resolution_idx]
        } else {
            "ignored; source format selected"
        }
    }

    fn handle_form_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => return true,
            KeyCode::Char('q') if !matches!(self.focus, Focus::Url | Focus::Output) => return true,
            KeyCode::Tab | KeyCode::Down => self.focus = self.focus.next(),
            KeyCode::BackTab | KeyCode::Up => self.focus = self.focus.previous(),
            KeyCode::Enter => {
                if self.focus == Focus::Start {
                    self.start_download();
                } else if self.focus == Focus::SourceFormat && self.available_formats.is_empty() {
                    self.load_formats();
                } else {
                    self.focus = self.focus.next();
                }
            }
            KeyCode::Char('f') if !matches!(self.focus, Focus::Url | Focus::Output) => {
                self.load_formats()
            }
            KeyCode::Left => self.adjust_selection(-1),
            KeyCode::Right => self.adjust_selection(1),
            KeyCode::Char(' ') if self.focus == Focus::Convert => self.convert = !self.convert,
            KeyCode::Backspace if self.focus == Focus::Url => {
                self.url.pop();
                self.clear_loaded_formats();
            }
            KeyCode::Backspace if self.focus == Focus::Output => {
                self.output_dir_input.pop();
                self.status = "Output directory changed.".to_string();
            }
            KeyCode::Char(ch) if self.focus == Focus::Url => {
                self.url.push(ch);
                self.clear_loaded_formats();
            }
            KeyCode::Char(ch) if self.focus == Focus::Output => {
                self.output_dir_input.push(ch);
                self.status = "Output directory changed.".to_string();
            }
            _ => {}
        }

        false
    }

    fn handle_running_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.request_cancel();
            }
            _ => {}
        }

        false
    }

    fn handle_done_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => true,
            KeyCode::Char('n') | KeyCode::Enter => {
                self.screen = Screen::Form;
                self.focus = Focus::Url;
                self.status = "Ready for another download.".to_string();
                self.progress = None;
                self.result_success = None;
                self.logs.clear();
                false
            }
            KeyCode::Char('o') => {
                if let Err(error) = open_output_dir(&self.output_dir) {
                    self.push_log(format!("Could not open folder: {error}"));
                }
                false
            }
            _ => false,
        }
    }

    fn adjust_selection(&mut self, direction: isize) {
        match self.focus {
            Focus::SourceFormat => {
                self.source_format_idx = cycle_index(
                    self.source_format_idx,
                    self.available_formats.len() + 1,
                    direction,
                );
            }
            Focus::Resolution => {
                self.resolution_idx =
                    cycle_index(self.resolution_idx, RESOLUTIONS.len(), direction);
            }
            Focus::Format => {
                self.format_idx = cycle_index(self.format_idx, FORMATS.len(), direction);
            }
            Focus::Convert => self.convert = !self.convert,
            Focus::Encoder => self.encoder_mode = self.encoder_mode.next(),
            _ => {}
        }
    }

    fn start_download(&mut self) {
        let url = self.url.trim().to_string();
        if url.is_empty() {
            self.status = "URL is required.".to_string();
            return;
        }

        if self.selected_source_format().is_some() && looks_like_playlist_only_url(&url) {
            self.status =
                "Source Format is single-video only. Use Auto best for playlist URLs.".to_string();
            return;
        }

        if let Some(source_format) = self.selected_source_format() {
            if let Some(error) =
                selected_format_container_error(source_format, FORMATS[self.format_idx])
            {
                self.status = error;
                self.focus = Focus::SourceFormat;
                return;
            }
        }

        let output_dir = match expand_output_dir(&self.output_dir_input) {
            Ok(path) => path,
            Err(error) => {
                self.status = error;
                self.focus = Focus::Output;
                return;
            }
        };

        self.cancel_format_loading();

        self.logs.clear();
        self.progress = Some(Progress {
            stage: "Preparing".to_string(),
            ratio: None,
            detail: "Starting yt-dlp...".to_string(),
        });
        self.status = "Download started.".to_string();
        self.output_dir = output_dir.clone();
        self.playlist_progress = None;
        self.result_success = None;
        self.screen = Screen::Running;

        let resolution = RESOLUTIONS[self.resolution_idx].to_string();
        let format = FORMATS[self.format_idx].to_string();
        let convert = self.convert;
        let encoder_mode = self.encoder_mode;
        let selected_source_format = self.selected_source_format().cloned();
        let control = ProcessControl::new();
        let job_control = control.clone();
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            run_download_job(
                tx,
                JobConfig {
                    url,
                    resolution,
                    format,
                    output_dir,
                    convert,
                    encoder_mode,
                    selected_source_format,
                    control: job_control,
                },
            );
        });

        self.worker_rx = Some(rx);
        self.worker_control = Some(control);
    }

    fn push_log(&mut self, line: impl Into<String>) {
        if self.logs.len() >= MAX_LOG_LINES {
            self.logs.pop_front();
        }
        self.logs.push_back(line.into());
    }

    fn drain_format_events(&mut self) {
        let Some(rx) = self.format_rx.as_ref() else {
            return;
        };

        match rx.try_recv() {
            Ok(Ok(formats)) => {
                let count = formats.len();
                self.available_formats = formats;
                self.source_format_idx = 0;
                self.formats_loading = false;
                self.format_rx = None;
                self.format_control = None;
                self.status =
                    format!("Loaded {count} available formats. Use Left/Right on Source Format.");
            }
            Ok(Err(error)) => {
                self.formats_loading = false;
                self.format_rx = None;
                self.format_control = None;
                self.status = error;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.formats_loading = false;
                self.format_rx = None;
                self.format_control = None;
                self.status = "Format loading stopped unexpectedly.".to_string();
            }
        }
    }

    fn load_formats(&mut self) {
        let url = self.url.trim().to_string();
        if url.is_empty() {
            self.status = "URL is required before loading formats.".to_string();
            return;
        }
        if looks_like_playlist_only_url(&url) {
            self.status =
                "Exact source format loading is single-video only. Use Auto best for playlists."
                    .to_string();
            return;
        }
        if self.formats_loading {
            self.status = "Formats are already loading.".to_string();
            return;
        }

        let (tx, rx) = mpsc::channel();
        let control = ProcessControl::new();
        let format_control = control.clone();
        self.formats_loading = true;
        self.format_rx = Some(rx);
        self.format_control = Some(control);
        self.status = "Loading available formats with yt-dlp...".to_string();

        thread::spawn(move || {
            let _ = tx.send(load_available_formats_with_control(&url, &format_control));
        });
    }

    fn selected_source_format(&self) -> Option<&AvailableFormat> {
        if self.source_format_idx == 0 {
            None
        } else {
            self.available_formats.get(self.source_format_idx - 1)
        }
    }

    fn clear_loaded_formats(&mut self) {
        if self.available_formats.is_empty() && !self.formats_loading {
            return;
        }

        if self.formats_loading {
            if let Some(control) = self.format_control.as_ref() {
                control.cancel();
            }
            self.status = "URL changed; format loading cancelled.".to_string();
        } else {
            self.status = "URL changed; load formats again if needed.".to_string();
        }

        self.available_formats.clear();
        self.source_format_idx = 0;
        self.formats_loading = false;
        self.format_rx = None;
        self.format_control = None;
    }

    fn cancel_format_loading(&mut self) {
        if !self.formats_loading {
            return;
        }

        if let Some(control) = self.format_control.as_ref() {
            control.cancel();
        }
        self.formats_loading = false;
        self.format_rx = None;
        self.format_control = None;
    }

    fn request_cancel(&mut self) {
        let Some(control) = self.worker_control.as_ref() else {
            self.push_log("No running process to cancel.");
            return;
        };

        if control.cancel() {
            self.status = "Cancellation requested.".to_string();
            self.push_log("Cancellation requested. Stopping the active process...");
        } else {
            self.status = "Cancellation already requested.".to_string();
        }

        let ratio = self.progress.as_ref().and_then(|progress| progress.ratio);
        self.progress = Some(Progress {
            stage: "Cancelling".to_string(),
            ratio,
            detail: "Waiting for the active process to stop...".to_string(),
        });
    }
}
