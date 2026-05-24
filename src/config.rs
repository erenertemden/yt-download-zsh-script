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

pub fn default_output_dir_input() -> String {
    default_output_dir().display().to_string()
}

pub fn expand_output_dir(input: &str) -> Result<PathBuf, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Output directory is required.".to_string());
    }

    if trimmed == "~" {
        return home_dir().ok_or_else(|| "Could not expand ~ because HOME is not set.".to_string());
    }

    if let Some(rest) = trimmed.strip_prefix("~/") {
        return home_dir()
            .map(|home| home.join(rest))
            .ok_or_else(|| "Could not expand ~/ because HOME is not set.".to_string());
    }

    Ok(PathBuf::from(trimmed))
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

fn home_dir() -> Option<PathBuf> {
    env::var("HOME").ok().map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_plain_output_path() {
        assert_eq!(
            expand_output_dir("/tmp/videos").expect("path should expand"),
            PathBuf::from("/tmp/videos")
        );
    }

    #[test]
    fn rejects_empty_output_path() {
        assert!(expand_output_dir("   ").is_err());
    }
}
