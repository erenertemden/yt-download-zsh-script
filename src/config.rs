use std::{env, path::PathBuf};

pub const RESOLUTIONS: &[&str] = &["Best", "1080", "720", "480"];
pub const FORMATS: &[&str] = &["mp4", "webm", "mkv"];
pub const MAX_LOG_LINES: usize = 300;

pub fn default_output_dir() -> PathBuf {
    if let Ok(home) = env::var("HOME") {
        PathBuf::from(home)
            .join("Downloads")
            .join("youtube_downloads")
    } else {
        PathBuf::from("youtube_downloads")
    }
}

pub fn cycle_index(current: usize, len: usize, direction: isize) -> usize {
    if len == 0 {
        return 0;
    }

    if direction < 0 {
        current.checked_sub(1).unwrap_or(len - 1)
    } else {
        (current + 1) % len
    }
}
