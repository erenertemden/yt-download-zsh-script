use std::{
    collections::{HashSet, VecDeque},
    env, fs,
    io::{self, BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread,
    time::{Duration, SystemTime},
};

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};

type AppResult<T> = Result<T, Box<dyn std::error::Error>>;

const RESOLUTIONS: &[&str] = &["Best", "1080", "720", "480"];
const FORMATS: &[&str] = &["mp4", "webm", "mkv"];
const MAX_LOG_LINES: usize = 300;

fn main() -> AppResult<()> {
    let mut terminal = setup_terminal()?;
    let result = run(&mut terminal);
    restore_terminal(&mut terminal)?;
    result
}

fn setup_terminal() -> AppResult<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> AppResult<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> AppResult<()> {
    let mut app = App::default();

    loop {
        app.drain_worker_events();
        terminal.draw(|frame| draw(frame, &app))?;

        if event::poll(Duration::from_millis(120))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && app.handle_key(key) {
                    return Ok(());
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Screen {
    Form,
    Running,
    Done,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Focus {
    Url,
    Resolution,
    Format,
    Convert,
    Encoder,
    Start,
}

impl Focus {
    fn next(self) -> Self {
        match self {
            Self::Url => Self::Resolution,
            Self::Resolution => Self::Format,
            Self::Format => Self::Convert,
            Self::Convert => Self::Encoder,
            Self::Encoder => Self::Start,
            Self::Start => Self::Url,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Url => Self::Start,
            Self::Resolution => Self::Url,
            Self::Format => Self::Resolution,
            Self::Convert => Self::Format,
            Self::Encoder => Self::Convert,
            Self::Start => Self::Encoder,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EncoderMode {
    AppleHardware,
    CpuX264,
}

impl Default for EncoderMode {
    fn default() -> Self {
        if cfg!(target_os = "macos") {
            Self::AppleHardware
        } else {
            Self::CpuX264
        }
    }
}

impl EncoderMode {
    fn label(self) -> &'static str {
        match self {
            Self::AppleHardware => "Fast Apple Hardware",
            Self::CpuX264 => "Smaller CPU x264",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::AppleHardware => Self::CpuX264,
            Self::CpuX264 => Self::AppleHardware,
        }
    }
}

#[derive(Clone, Debug)]
struct Progress {
    stage: String,
    ratio: Option<f64>,
    detail: String,
}

#[derive(Debug)]
enum WorkerEvent {
    Log(String),
    Progress(Progress),
    Done { success: bool, message: String },
}

struct App {
    screen: Screen,
    focus: Focus,
    url: String,
    resolution_idx: usize,
    format_idx: usize,
    convert: bool,
    encoder_mode: EncoderMode,
    output_dir: PathBuf,
    logs: VecDeque<String>,
    progress: Option<Progress>,
    status: String,
    worker_rx: Option<Receiver<WorkerEvent>>,
    result_success: Option<bool>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            screen: Screen::Form,
            focus: Focus::Url,
            url: String::new(),
            resolution_idx: 0,
            format_idx: 0,
            convert: true,
            encoder_mode: EncoderMode::default(),
            output_dir: default_output_dir(),
            logs: VecDeque::new(),
            progress: None,
            status: "Paste a video or playlist URL.".to_string(),
            worker_rx: None,
            result_success: None,
        }
    }
}

impl App {
    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return self.screen != Screen::Running;
        }

        match self.screen {
            Screen::Form => self.handle_form_key(key),
            Screen::Running => self.handle_running_key(key),
            Screen::Done => self.handle_done_key(key),
        }
    }

    fn handle_form_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return true,
            KeyCode::Tab | KeyCode::Down => self.focus = self.focus.next(),
            KeyCode::BackTab | KeyCode::Up => self.focus = self.focus.previous(),
            KeyCode::Enter => {
                if self.focus == Focus::Start {
                    self.start_download();
                } else {
                    self.focus = self.focus.next();
                }
            }
            KeyCode::Left => self.adjust_selection(-1),
            KeyCode::Right => self.adjust_selection(1),
            KeyCode::Char(' ') if self.focus == Focus::Convert => self.convert = !self.convert,
            KeyCode::Backspace if self.focus == Focus::Url => {
                self.url.pop();
            }
            KeyCode::Char(ch) if self.focus == Focus::Url => self.url.push(ch),
            _ => {}
        }

        false
    }

    fn handle_running_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.push_log("Download is running; wait for completion before quitting.");
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

        self.logs.clear();
        self.progress = Some(Progress {
            stage: "Preparing".to_string(),
            ratio: None,
            detail: "Starting yt-dlp...".to_string(),
        });
        self.status = "Download started.".to_string();
        self.result_success = None;
        self.screen = Screen::Running;

        let resolution = RESOLUTIONS[self.resolution_idx].to_string();
        let format = FORMATS[self.format_idx].to_string();
        let output_dir = self.output_dir.clone();
        let convert = self.convert;
        let encoder_mode = self.encoder_mode;
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
                },
            );
        });

        self.worker_rx = Some(rx);
    }

    fn drain_worker_events(&mut self) {
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
                WorkerEvent::Done { success, message } => {
                    self.result_success = Some(success);
                    self.status = message.clone();
                    self.push_log(message);
                    self.screen = Screen::Done;
                    self.worker_rx = None;
                }
            }
        }

        if disconnected && self.screen == Screen::Running {
            self.result_success = Some(false);
            self.status = "Worker stopped unexpectedly.".to_string();
            self.push_log("Worker stopped unexpectedly.");
            self.screen = Screen::Done;
            self.worker_rx = None;
        }
    }

    fn push_log(&mut self, line: impl Into<String>) {
        if self.logs.len() >= MAX_LOG_LINES {
            self.logs.pop_front();
        }
        self.logs.push_back(line.into());
    }
}

#[derive(Clone)]
struct JobConfig {
    url: String,
    resolution: String,
    format: String,
    output_dir: PathBuf,
    convert: bool,
    encoder_mode: EncoderMode,
}

fn run_download_job(tx: Sender<WorkerEvent>, config: JobConfig) {
    if let Err(error) = run_download_job_inner(&tx, &config) {
        let _ = tx.send(WorkerEvent::Done {
            success: false,
            message: error,
        });
    }
}

fn run_download_job_inner(tx: &Sender<WorkerEvent>, config: &JobConfig) -> Result<(), String> {
    fs::create_dir_all(&config.output_dir)
        .map_err(|error| format!("Could not create output directory: {error}"))?;

    let started_at = SystemTime::now();
    let before = snapshot_files(&config.output_dir, &config.format);

    send_log(tx, format!("Output: {}", config.output_dir.display()));
    send_log(tx, format!("Resolution: {}", config.resolution));
    send_log(tx, format!("Format: {}", config.format));
    if config.convert {
        send_log(tx, format!("Encoder: {}", config.encoder_mode.label()));
    }

    let selector = format_selector(&config.format, &config.resolution);
    let output_template = config.output_dir.join("%(title)s.%(ext)s");

    let mut child = Command::new("yt-dlp")
        .arg("--newline")
        .arg("--progress")
        .arg("--yes-playlist")
        .arg("--merge-output-format")
        .arg(&config.format)
        .arg("--remux-video")
        .arg(&config.format)
        .arg("-f")
        .arg(selector)
        .arg("-o")
        .arg(output_template)
        .arg(&config.url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Could not start yt-dlp: {error}"))?;

    stream_child_output(&mut child, Sender::clone(tx), OutputKind::YtDlp)?;

    let status = child
        .wait()
        .map_err(|error| format!("yt-dlp failed to finish: {error}"))?;

    if !status.success() {
        return Err(format!("yt-dlp exited with status: {status}"));
    }

    send_log(tx, "Download completed.");

    if config.convert {
        let files = new_downloaded_files(&config.output_dir, &config.format, &before, started_at);
        convert_files(tx, files, config.encoder_mode)?;
    }

    let message = if config.convert {
        "Completed. Downloaded files were converted for QuickTime.".to_string()
    } else {
        "Completed. Downloaded files were kept in the selected format.".to_string()
    };

    let _ = tx.send(WorkerEvent::Done {
        success: true,
        message,
    });
    Ok(())
}

fn stream_child_output(
    child: &mut std::process::Child,
    tx: Sender<WorkerEvent>,
    kind: OutputKind,
) -> Result<(), String> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Could not capture process stdout.".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Could not capture process stderr.".to_string())?;

    spawn_reader(stdout, tx.clone(), kind);
    spawn_reader(stderr, tx, kind);
    Ok(())
}

#[derive(Clone, Copy)]
enum OutputKind {
    YtDlp,
}

fn spawn_reader<R>(reader: R, tx: Sender<WorkerEvent>, kind: OutputKind)
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let reader = BufReader::new(reader);
        for line in reader.lines() {
            let Ok(line) = line else {
                break;
            };
            if line.trim().is_empty() {
                continue;
            }

            if let Some(progress) = match kind {
                OutputKind::YtDlp => parse_ytdlp_progress(&line),
            } {
                let _ = tx.send(WorkerEvent::Progress(progress));
            }

            let _ = tx.send(WorkerEvent::Log(line));
        }
    });
}

fn convert_files(
    tx: &Sender<WorkerEvent>,
    files: Vec<PathBuf>,
    encoder_mode: EncoderMode,
) -> Result<(), String> {
    if files.is_empty() {
        send_log(
            tx,
            "No new files matched the selected format; skipping conversion.",
        );
        return Ok(());
    }

    let requested_encoder_mode = encoder_mode;
    let encoder_mode = effective_encoder_mode(encoder_mode);
    if encoder_mode != requested_encoder_mode {
        send_log(
            tx,
            "Apple hardware encoding is only available on macOS; using CPU x264 instead.",
        );
    }
    send_log(tx, format!("Converting with {}", encoder_mode.label()));

    for (index, file) in files.iter().enumerate() {
        let output = fixed_mp4_path(file);
        let detail = format!(
            "{} -> {}",
            file.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("video"),
            output
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("fixed-video.mp4")
        );
        let duration_secs = probe_duration(file);
        if duration_secs.is_none() {
            send_log(
                tx,
                format!(
                    "Could not determine duration for {}; conversion progress will update when the file finishes.",
                    file.file_name().and_then(|name| name.to_str()).unwrap_or("video")
                ),
            );
        }

        let _ = tx.send(WorkerEvent::Progress(Progress {
            stage: format!("Converting {}/{}", index + 1, files.len()),
            ratio: Some(index as f64 / files.len() as f64),
            detail: detail.clone(),
        }));

        let mut command = Command::new("ffmpeg");
        command
            .arg("-y")
            .arg("-nostats")
            .arg("-progress")
            .arg("pipe:1")
            .arg("-i")
            .arg(file);
        append_video_encoder_args(&mut command, encoder_mode);

        let mut child = command
            .arg("-c:a")
            .arg("aac")
            .arg("-b:a")
            .arg("128k")
            .arg("-movflags")
            .arg("+faststart")
            .arg(&output)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("Could not start ffmpeg: {error}"))?;

        stream_ffmpeg_output(
            &mut child,
            Sender::clone(tx),
            FfmpegProgressContext {
                file_index: index,
                total_files: files.len(),
                duration_secs,
                detail,
            },
        )?;

        let status = child
            .wait()
            .map_err(|error| format!("ffmpeg failed to finish: {error}"))?;

        if !status.success() {
            return Err(format!("ffmpeg exited with status: {status}"));
        }

        let _ = tx.send(WorkerEvent::Progress(Progress {
            stage: format!("Converting {}/{}", index + 1, files.len()),
            ratio: Some((index + 1) as f64 / files.len() as f64),
            detail: "File converted.".to_string(),
        }));
    }

    let _ = tx.send(WorkerEvent::Progress(Progress {
        stage: "Converting".to_string(),
        ratio: Some(1.0),
        detail: "All files converted.".to_string(),
    }));

    Ok(())
}

fn effective_encoder_mode(encoder_mode: EncoderMode) -> EncoderMode {
    if encoder_mode == EncoderMode::AppleHardware && !cfg!(target_os = "macos") {
        EncoderMode::CpuX264
    } else {
        encoder_mode
    }
}

fn append_video_encoder_args(command: &mut Command, encoder_mode: EncoderMode) {
    match encoder_mode {
        EncoderMode::AppleHardware => {
            command
                .arg("-c:v")
                .arg("h264_videotoolbox")
                .arg("-b:v")
                .arg("6000k")
                .arg("-pix_fmt")
                .arg("yuv420p");
        }
        EncoderMode::CpuX264 => {
            command
                .arg("-c:v")
                .arg("libx264")
                .arg("-preset")
                .arg("fast")
                .arg("-crf")
                .arg("23")
                .arg("-pix_fmt")
                .arg("yuv420p");
        }
    }
}

#[derive(Clone)]
struct FfmpegProgressContext {
    file_index: usize,
    total_files: usize,
    duration_secs: Option<f64>,
    detail: String,
}

fn stream_ffmpeg_output(
    child: &mut std::process::Child,
    tx: Sender<WorkerEvent>,
    context: FfmpegProgressContext,
) -> Result<(), String> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Could not capture ffmpeg stdout.".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Could not capture ffmpeg stderr.".to_string())?;

    spawn_ffmpeg_progress_reader(stdout, tx.clone(), context);
    spawn_ffmpeg_log_reader(stderr, tx);
    Ok(())
}

fn spawn_ffmpeg_progress_reader<R>(
    reader: R,
    tx: Sender<WorkerEvent>,
    context: FfmpegProgressContext,
) where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let reader = BufReader::new(reader);
        let stage = format!(
            "Converting {}/{}",
            context.file_index + 1,
            context.total_files
        );

        for line in reader.lines() {
            let Ok(line) = line else {
                break;
            };
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if line == "progress=end" {
                let _ = tx.send(WorkerEvent::Progress(Progress {
                    stage: stage.clone(),
                    ratio: Some((context.file_index + 1) as f64 / context.total_files as f64),
                    detail: context.detail.clone(),
                }));
                continue;
            }

            let Some(duration_secs) = context.duration_secs else {
                continue;
            };
            let Some(out_time_secs) = parse_ffmpeg_out_time(line) else {
                continue;
            };

            let file_ratio = (out_time_secs / duration_secs).clamp(0.0, 1.0);
            let overall_ratio =
                (context.file_index as f64 + file_ratio) / context.total_files as f64;
            let detail = format!(
                "{} ({}/{})",
                context.detail,
                format_duration(out_time_secs),
                format_duration(duration_secs)
            );

            let _ = tx.send(WorkerEvent::Progress(Progress {
                stage: stage.clone(),
                ratio: Some(overall_ratio),
                detail,
            }));
        }
    });
}

fn spawn_ffmpeg_log_reader<R>(reader: R, tx: Sender<WorkerEvent>)
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let reader = BufReader::new(reader);
        for line in reader.lines() {
            let Ok(line) = line else {
                break;
            };
            if line.trim().is_empty() {
                continue;
            }
            let _ = tx.send(WorkerEvent::Log(line));
        }
    });
}

fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(frame, app, chunks[0]);

    match app.screen {
        Screen::Form => draw_form(frame, app, chunks[1]),
        Screen::Running => draw_running(frame, app, chunks[1]),
        Screen::Done => draw_done(frame, app, chunks[1]),
    }

    draw_help(frame, app, chunks[2]);
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let title = Line::from(vec![
        Span::styled(
            "YouTube Downloader TUI",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(&app.status, Style::default().fg(Color::Gray)),
    ]);

    let header = Paragraph::new(title)
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Left);
    frame.render_widget(header, area);
}

fn draw_form(frame: &mut Frame, app: &App, area: Rect) {
    let rows = vec![
        selectable_line(
            app.focus == Focus::Url,
            "URL",
            if app.url.is_empty() {
                "paste video or playlist URL"
            } else {
                &app.url
            },
        ),
        selectable_line(
            app.focus == Focus::Resolution,
            "Resolution",
            RESOLUTIONS[app.resolution_idx],
        ),
        selectable_line(
            app.focus == Focus::Format,
            "Format",
            FORMATS[app.format_idx],
        ),
        selectable_line(
            app.focus == Focus::Convert,
            "QuickTime mp4",
            if app.convert { "enabled" } else { "disabled" },
        ),
        selectable_line(
            app.focus == Focus::Encoder,
            "Encoder",
            if app.convert {
                app.encoder_mode.label()
            } else {
                "not used"
            },
        ),
        selectable_line(
            false,
            "Output",
            app.output_dir
                .to_str()
                .unwrap_or("~/Downloads/youtube_downloads"),
        ),
        Line::raw(""),
        button_line(app.focus == Focus::Start, "Start Download"),
    ];

    let paragraph = Paragraph::new(rows)
        .block(Block::default().title("Download").borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn draw_running(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(6)])
        .split(area);

    let progress = app.progress.clone().unwrap_or(Progress {
        stage: "Running".to_string(),
        ratio: None,
        detail: "Waiting for output...".to_string(),
    });
    let ratio = progress.ratio.unwrap_or(0.0).clamp(0.0, 1.0);
    let label = match progress.ratio {
        Some(value) => format!("{} {:>3.0}%", progress.stage, value * 100.0),
        None => progress.stage,
    };
    let gauge = Gauge::default()
        .block(
            Block::default()
                .title(progress.detail)
                .borders(Borders::ALL),
        )
        .gauge_style(Style::default().fg(Color::Green).bg(Color::Black))
        .ratio(ratio)
        .label(label);
    frame.render_widget(gauge, chunks[0]);

    draw_logs(frame, app, chunks[1]);
}

fn draw_done(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(6), Constraint::Min(6)])
        .split(area);

    let success = app.result_success.unwrap_or(false);
    let color = if success { Color::Green } else { Color::Red };
    let summary = vec![
        Line::from(Span::styled(
            if success { "Completed" } else { "Failed" },
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )),
        Line::raw(app.status.clone()),
        Line::raw(format!("Saved to: {}", app.output_dir.display())),
    ];
    let paragraph = Paragraph::new(summary)
        .block(Block::default().title("Result").borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, chunks[0]);

    draw_logs(frame, app, chunks[1]);
}

fn draw_logs(frame: &mut Frame, app: &App, area: Rect) {
    let height = area.height.saturating_sub(2) as usize;
    let start = app.logs.len().saturating_sub(height);
    let items: Vec<ListItem> = app
        .logs
        .iter()
        .skip(start)
        .map(|line| ListItem::new(Line::raw(line.clone())))
        .collect();

    let list = List::new(items).block(Block::default().title("Logs").borders(Borders::ALL));
    frame.render_widget(list, area);
}

fn draw_help(frame: &mut Frame, app: &App, area: Rect) {
    let text = match app.screen {
        Screen::Form => {
            "Tab/Up/Down focus  Left/Right choose  Space toggle  Enter confirm/start  q quit"
        }
        Screen::Running => {
            "Download is running. Logs update live; q/Esc is disabled until the job finishes."
        }
        Screen::Done => "Enter/n new download  o open folder  q/Esc quit",
    };
    let help = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, area);
}

fn selectable_line<'a>(selected: bool, label: &'a str, value: &'a str) -> Line<'a> {
    let marker = if selected { "> " } else { "  " };
    let value_style = if selected {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };

    Line::from(vec![
        Span::raw(marker),
        Span::styled(format!("{label:<14}"), Style::default().fg(Color::Gray)),
        Span::styled(value, value_style),
    ])
}

fn button_line<'a>(selected: bool, label: &'a str) -> Line<'a> {
    let style = if selected {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green)
    };

    Line::from(vec![
        Span::raw(if selected { "> " } else { "  " }),
        Span::styled(format!("[ {label} ]"), style),
    ])
}

fn default_output_dir() -> PathBuf {
    if let Ok(home) = env::var("HOME") {
        PathBuf::from(home)
            .join("Downloads")
            .join("youtube_downloads")
    } else {
        PathBuf::from("youtube_downloads")
    }
}

fn cycle_index(current: usize, len: usize, direction: isize) -> usize {
    if len == 0 {
        return 0;
    }

    if direction < 0 {
        current.checked_sub(1).unwrap_or(len - 1)
    } else {
        (current + 1) % len
    }
}

fn format_selector(_format: &str, resolution: &str) -> String {
    if resolution == "Best" {
        "bestvideo+bestaudio/best".to_string()
    } else {
        format!("bestvideo[height<={resolution}]+bestaudio/best[height<={resolution}]")
    }
}

fn parse_ytdlp_progress(line: &str) -> Option<Progress> {
    if !line.contains("[download]") {
        return None;
    }

    let percent = parse_percent(line)?;
    let detail = compact_download_detail(line);
    Some(Progress {
        stage: "Downloading".to_string(),
        ratio: Some(percent / 100.0),
        detail,
    })
}

fn parse_percent(line: &str) -> Option<f64> {
    let percent_pos = line.find('%')?;
    let prefix = &line[..percent_pos];
    let start = prefix
        .rfind(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
        .map_or(0, |idx| idx + 1);
    prefix[start..].trim().parse::<f64>().ok()
}

fn compact_download_detail(line: &str) -> String {
    let trimmed = line.trim();
    if let Some(start) = trimmed.find(" at ") {
        return trimmed[start + 4..].to_string();
    }
    trimmed.to_string()
}

fn probe_duration(file: &Path) -> Option<f64> {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-show_entries")
        .arg("format=duration")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .arg(file)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let duration = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<f64>()
        .ok()?;

    duration
        .is_finite()
        .then_some(duration)
        .filter(|value| *value > 0.0)
}

fn parse_ffmpeg_out_time(line: &str) -> Option<f64> {
    if let Some(value) = line.strip_prefix("out_time=") {
        return parse_hms_time(value);
    }

    if let Some(value) = line.strip_prefix("out_time_us=") {
        return parse_progress_microseconds(value);
    }

    if let Some(value) = line.strip_prefix("out_time_ms=") {
        return parse_progress_microseconds(value);
    }

    None
}

fn parse_progress_microseconds(value: &str) -> Option<f64> {
    let micros = value.trim().parse::<f64>().ok()?;
    micros.is_finite().then_some(micros / 1_000_000.0)
}

fn parse_hms_time(value: &str) -> Option<f64> {
    let mut parts = value.trim().split(':');
    let hours = parts.next()?.parse::<f64>().ok()?;
    let minutes = parts.next()?.parse::<f64>().ok()?;
    let seconds = parts.next()?.parse::<f64>().ok()?;

    if parts.next().is_some() || !hours.is_finite() || !minutes.is_finite() || !seconds.is_finite()
    {
        return None;
    }

    Some(hours * 3600.0 + minutes * 60.0 + seconds)
}

fn format_duration(seconds: f64) -> String {
    let total_seconds = seconds.max(0.0).round() as u64;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

fn snapshot_files(dir: &Path, ext: &str) -> HashSet<PathBuf> {
    read_files_with_extension(dir, ext)
        .unwrap_or_default()
        .into_iter()
        .collect()
}

fn new_downloaded_files(
    dir: &Path,
    ext: &str,
    before: &HashSet<PathBuf>,
    started_at: SystemTime,
) -> Vec<PathBuf> {
    let mut files = read_files_with_extension(dir, ext).unwrap_or_default();
    files.retain(|path| {
        if !before.contains(path) {
            return true;
        }

        path.metadata()
            .and_then(|metadata| metadata.modified())
            .map(|modified| modified >= started_at)
            .unwrap_or(false)
    });
    files.sort();
    files
}

fn read_files_with_extension(dir: &Path, ext: &str) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !dir.exists() {
        return Ok(files);
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case(ext))
        {
            files.push(path);
        }
    }

    Ok(files)
}

fn fixed_mp4_path(input: &Path) -> PathBuf {
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    let stem = input
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("video");
    parent.join(format!("fixed-{stem}.mp4"))
}

fn send_log(tx: &Sender<WorkerEvent>, line: impl Into<String>) {
    let _ = tx.send(WorkerEvent::Log(line.into()));
}

fn open_output_dir(path: &Path) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn()?;
    }

    #[cfg(not(target_os = "macos"))]
    {
        Command::new("xdg-open").arg(path).spawn()?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ytdlp_percent() {
        assert_eq!(
            parse_percent("[download]  42.5% of 10.00MiB at 1.00MiB/s ETA 00:05"),
            Some(42.5)
        );
    }

    #[test]
    fn parses_ffmpeg_out_time() {
        assert_eq!(
            parse_ffmpeg_out_time("out_time=00:01:02.500000"),
            Some(62.5)
        );
        assert_eq!(parse_ffmpeg_out_time("out_time_us=2500000"), Some(2.5));
    }

    #[test]
    fn formats_duration() {
        assert_eq!(format_duration(62.4), "1:02");
        assert_eq!(format_duration(3661.0), "1:01:01");
    }
}
