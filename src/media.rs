use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};

use serde_json::Value;

use crate::types::{AvailableFormat, EncoderMode, Progress};

pub fn load_available_formats(url: &str) -> Result<Vec<AvailableFormat>, String> {
    let output = Command::new("yt-dlp")
        .arg("-J")
        .arg("--no-playlist")
        .arg(url)
        .output()
        .map_err(|error| format!("Could not start yt-dlp: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Could not load formats: {}",
            stderr.trim().lines().last().unwrap_or("yt-dlp failed")
        ));
    }

    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Could not parse yt-dlp JSON: {error}"))?;
    let formats = extract_format_values(&value)
        .ok_or_else(|| "yt-dlp did not return a format list for this URL.".to_string())?;

    let mut rows = Vec::new();
    for format in formats {
        let Some(row) = parse_available_format(format) else {
            continue;
        };
        rows.push(row);
    }

    rows.sort_by(|left, right| {
        right
            .height
            .cmp(&left.height)
            .then_with(|| compare_optional_fps(&right.fps_label, &left.fps_label))
            .then_with(|| left.ext.cmp(&right.ext))
            .then_with(|| left.id.cmp(&right.id))
    });

    if rows.is_empty() {
        return Err("No selectable video formats were found.".to_string());
    }

    Ok(rows)
}

pub fn format_selector(_format: &str, resolution: &str) -> String {
    if resolution == "Best" {
        "bestvideo+bestaudio/best".to_string()
    } else {
        format!("bestvideo[height<={resolution}]+bestaudio/best[height<={resolution}]")
    }
}

pub fn selected_format_selector(format: &AvailableFormat) -> String {
    if format.has_audio {
        format!("{}/best", format.id)
    } else {
        format!("{}+bestaudio/best", format.id)
    }
}

pub fn parse_ytdlp_progress(line: &str) -> Option<Progress> {
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

pub fn probe_duration(file: &Path) -> Option<f64> {
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

pub fn parse_ffmpeg_out_time(line: &str) -> Option<f64> {
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

pub fn format_duration(seconds: f64) -> String {
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

pub fn snapshot_files(dir: &Path, ext: &str) -> HashSet<PathBuf> {
    read_files_with_extension(dir, ext)
        .unwrap_or_default()
        .into_iter()
        .collect()
}

pub fn new_downloaded_files(
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

pub fn fixed_mp4_path(input: &Path) -> PathBuf {
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    let stem = input
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("video");
    parent.join(format!("fixed-{stem}.mp4"))
}

pub fn effective_encoder_mode(encoder_mode: EncoderMode) -> EncoderMode {
    if encoder_mode == EncoderMode::AppleHardware && !cfg!(target_os = "macos") {
        EncoderMode::CpuX264
    } else {
        encoder_mode
    }
}

pub fn append_video_encoder_args(command: &mut Command, encoder_mode: EncoderMode) {
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

fn extract_format_values(value: &Value) -> Option<&Vec<Value>> {
    if let Some(formats) = value.get("formats").and_then(Value::as_array) {
        return Some(formats);
    }

    value
        .get("entries")
        .and_then(Value::as_array)
        .and_then(|entries| {
            entries
                .iter()
                .find_map(|entry| entry.get("formats")?.as_array())
        })
}

fn parse_available_format(value: &Value) -> Option<AvailableFormat> {
    let id = value.get("format_id")?.as_str()?.to_string();
    let ext = value
        .get("ext")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let vcodec = value
        .get("vcodec")
        .and_then(Value::as_str)
        .unwrap_or("none");
    if vcodec == "none" {
        return None;
    }

    let acodec = value
        .get("acodec")
        .and_then(Value::as_str)
        .unwrap_or("none");
    let has_audio = acodec != "none";
    let height = value
        .get("height")
        .and_then(Value::as_u64)
        .and_then(|height| u32::try_from(height).ok());
    let fps = value.get("fps").and_then(Value::as_f64);
    let fps_label = fps.map(format_fps);
    let codec = short_codec(vcodec);
    let size = format_size(value);
    let audio = if has_audio {
        "with audio"
    } else {
        "video-only"
    };
    let quality = match (height, fps_label.as_deref()) {
        (Some(height), Some(fps)) => format!("{height}p{fps}"),
        (Some(height), None) => format!("{height}p"),
        (None, Some(fps)) => format!("{fps}fps"),
        (None, None) => "unknown".to_string(),
    };
    let mut label = format!("{quality} {ext} {codec} {audio} id={id}");
    if let Some(size) = size {
        label.push_str(&format!(" {size}"));
    }

    Some(AvailableFormat {
        id,
        label,
        ext,
        height,
        fps_label,
        has_audio,
    })
}

fn compare_optional_fps(left: &Option<String>, right: &Option<String>) -> std::cmp::Ordering {
    parse_fps_label(left)
        .partial_cmp(&parse_fps_label(right))
        .unwrap_or(std::cmp::Ordering::Equal)
}

fn parse_fps_label(value: &Option<String>) -> f64 {
    value
        .as_deref()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn format_fps(fps: f64) -> String {
    if (fps.fract()).abs() < f64::EPSILON {
        format!("{fps:.0}")
    } else {
        format!("{fps:.2}")
    }
}

fn short_codec(codec: &str) -> &str {
    codec.split('.').next().unwrap_or(codec)
}

fn format_size(value: &Value) -> Option<String> {
    let bytes = value
        .get("filesize")
        .or_else(|| value.get("filesize_approx"))
        .and_then(Value::as_u64)?;
    Some(format!("~{:.1}MiB", bytes as f64 / 1024.0 / 1024.0))
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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

    #[test]
    fn parses_selectable_video_format() {
        let value = json!({
            "format_id": "299",
            "ext": "mp4",
            "vcodec": "avc1.64002a",
            "acodec": "none",
            "height": 1080,
            "fps": 60,
            "filesize_approx": 10485760
        });

        let format = parse_available_format(&value).expect("format should parse");

        assert_eq!(format.id, "299");
        assert_eq!(format.height, Some(1080));
        assert_eq!(format.fps_label.as_deref(), Some("60"));
        assert!(!format.has_audio);
        assert_eq!(selected_format_selector(&format), "299+bestaudio/best");
    }

    #[test]
    fn keeps_combined_format_selector_simple() {
        let value = json!({
            "format_id": "18",
            "ext": "mp4",
            "vcodec": "avc1.42001E",
            "acodec": "mp4a.40.2",
            "height": 360,
            "fps": 30
        });

        let format = parse_available_format(&value).expect("format should parse");

        assert!(format.has_audio);
        assert_eq!(selected_format_selector(&format), "18/best");
    }
}
