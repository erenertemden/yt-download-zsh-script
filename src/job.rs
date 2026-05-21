use std::{
    fs,
    io::{BufRead, BufReader, Read},
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc::Sender,
    thread,
    time::SystemTime,
};

use crate::{
    media::{
        append_video_encoder_args, effective_encoder_mode, fixed_mp4_path, format_duration,
        format_selector, new_downloaded_files, parse_ffmpeg_out_time, parse_ytdlp_progress,
        probe_duration, selected_format_selector, snapshot_files,
    },
    types::{AvailableFormat, EncoderMode, Progress, WorkerEvent},
};

#[derive(Clone)]
pub struct JobConfig {
    pub url: String,
    pub resolution: String,
    pub format: String,
    pub output_dir: PathBuf,
    pub convert: bool,
    pub encoder_mode: EncoderMode,
    pub selected_source_format: Option<AvailableFormat>,
}

pub fn run_download_job(tx: Sender<WorkerEvent>, config: JobConfig) {
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
    if let Some(format) = config.selected_source_format.as_ref() {
        send_log(tx, format!("Source format: {}", format.label));
    }
    if config.convert {
        send_log(tx, format!("Encoder: {}", config.encoder_mode.label()));
    }

    let selector = config
        .selected_source_format
        .as_ref()
        .map(selected_format_selector)
        .unwrap_or_else(|| format_selector(&config.format, &config.resolution));
    let output_template = config.output_dir.join("%(title)s.%(ext)s");

    let mut command = Command::new("yt-dlp");
    command
        .arg("--newline")
        .arg("--progress")
        .arg(if config.selected_source_format.is_some() {
            "--no-playlist"
        } else {
            "--yes-playlist"
        })
        .arg("--merge-output-format")
        .arg(&config.format)
        .arg("--remux-video")
        .arg(&config.format)
        .arg("-f")
        .arg(selector)
        .arg("-o")
        .arg(output_template)
        .arg(&config.url);

    let mut child = command
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

fn send_log(tx: &Sender<WorkerEvent>, line: impl Into<String>) {
    let _ = tx.send(WorkerEvent::Log(line.into()));
}
