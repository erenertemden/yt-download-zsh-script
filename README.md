# YouTube Downloader TUI for macOS

![Screenshot](screenshot.png)

This project includes a **macOS-first Ratatui terminal UI** for downloading YouTube videos or playlists with `yt-dlp`, then optionally converting the results to QuickTime-friendly `.mp4` files with `ffmpeg`.

The main goal is a simple Terminal.app-friendly workflow: paste a URL, choose quality, download, and get files that open cleanly in QuickTime Player and Finder.

The original Zsh script is still available as `yt-download-zsh-script.sh`.

---

## Current Status

This is currently a source-based Rust app for macOS. That means the easiest way to try it right now is:

```bash
cargo run
```

There is not yet a Homebrew formula, GitHub release binary, or `cargo install` package. Those can be added later so Apple Silicon and Intel Mac users can install the app without building it manually.

The current TUI is an MVP with:

- URL input
- Available source format loading from `yt-dlp`
- Resolution selection
- Output format selection
- QuickTime conversion toggle
- Encoder selection for fast Apple hardware encoding or smaller CPU x264 output
- Live `yt-dlp` / `ffmpeg` logs
- Download and conversion progress when the tools report timing output
- Result screen with new-download and open-folder actions

---

## Features

- macOS-first terminal workflow for Terminal.app, iTerm2, and similar terminal emulators
- Ratatui form for URL, source format, container, QuickTime conversion, and encoder mode
- Supports single videos and playlists through `yt-dlp`
- Live terminal logs with download and conversion progress
- Optional conversion to `fixed-*.mp4` for QuickTime Player compatibility
- Result screen action to open the download folder in Finder
- Saves downloads to `~/Downloads/youtube_downloads`

---

## Project Structure

The Rust app is split by responsibility so new features can be added without growing a single large `main.rs`:

```text
src/
  main.rs       app entrypoint and module wiring
  terminal.rs   terminal setup, teardown, and event loop
  app.rs        application state, keyboard input, and worker events
  ui.rs         Ratatui rendering
  job.rs        yt-dlp and ffmpeg worker process orchestration
  media.rs      available-format parsing, progress parsing, file discovery, encoder args
  types.rs      shared enums/events/progress models
  config.rs     constants and default paths
  system.rs     platform-specific helpers such as opening the output folder
```

Suggested ownership for future changes:

- Add new form fields in `types.rs`, `app.rs`, and `ui.rs`.
- Change download or conversion behavior in `job.rs`.
- Change ffmpeg/yt-dlp parsing, available-format parsing, or encoder details in `media.rs`.
- Change terminal lifecycle behavior in `terminal.rs`.

---

## macOS Compatibility

This project is designed around common macOS defaults:

- Downloads go to `~/Downloads/youtube_downloads`.
- QuickTime conversion creates H.264/AAC `.mp4` files, which are friendly to QuickTime Player, Finder preview, and AirDrop workflows.
- Apple Hardware mode uses FFmpeg's VideoToolbox encoder on macOS for faster conversion on Apple Silicon and supported Intel Macs.
- The result screen can open the output folder with the macOS `open` command.
- Homebrew is the recommended way to install Rust, `yt-dlp`, and `ffmpeg`.
- The intended release targets are Apple Silicon (`aarch64-apple-darwin`) and Intel Macs (`x86_64-apple-darwin`).

The app may also work on Linux because Ratatui, `yt-dlp`, and `ffmpeg` are cross-platform, but the current documentation and defaults are optimized for macOS.

---

## Requirements

- Rust toolchain (`cargo`, `rustc`)
- [yt-dlp](https://github.com/yt-dlp/yt-dlp)
- [ffmpeg](https://ffmpeg.org/) including `ffprobe`
- macOS Terminal, iTerm2, or another compatible terminal emulator

On macOS, install dependencies with Homebrew:

```bash
brew install rust yt-dlp ffmpeg
```

Verify that the required commands are available:

```bash
cargo --version
yt-dlp --version
ffmpeg -version
ffprobe -version
```

---

## Installation From Source

Clone the repository:

```bash
git clone https://github.com/erenertemden/yt-download-zsh-script.git
cd yt-download-zsh-script
```

Run the TUI in development mode:

```bash
cargo run
```

Build a release binary:

```bash
cargo build --release
./target/release/yt-download-tui
```

Optionally move the release binary somewhere in your `PATH`:

```bash
sudo cp ./target/release/yt-download-tui /usr/local/bin/yt-download-tui
yt-download-tui
```

---

## TUI Usage

Start the app:

```bash
cargo run
```

The app opens a terminal UI with these fields:

- `URL`: YouTube video or playlist URL
- `Source Format`: `Auto best` or a format loaded from the current URL
- `Fallback Res`: `Best`, `1080`, `720`, or `480`; used only when `Source Format` is `Auto best`
- `Container`: `mp4`, `webm`, or `mkv`
- `QuickTime mp4`: whether to create macOS-friendly `fixed-*.mp4` files after download
- `Encoder`: `Fast Apple Hardware` or `Smaller CPU x264`
- `Output`: currently fixed to `~/Downloads/youtube_downloads`

Controls inside the TUI:

- `Tab`, `Up`, `Down`: move between fields
- `f`: load available formats for the current URL
- `Left`, `Right`: change source format, resolution, container, or encoder mode
- `Space`: toggle QuickTime conversion
- `Enter`: confirm/start, or start a new download on the result screen
- `o`: open output folder on the result screen
- `q`, `Esc`: quit when idle

The TUI flow:

1. Paste a YouTube video or playlist URL.
2. Press `f` to load the formats that `yt-dlp` reports for that URL.
3. Choose `Source Format`, or leave it on `Auto best`.
4. Choose fallback resolution: `Best`, `1080`, `720`, or `480`; this is ignored when an exact source format is selected.
5. Choose output container: `mp4`, `webm`, or `mkv`.
6. Decide whether to create QuickTime-compatible `fixed-*.mp4` files for macOS playback.
7. Choose an encoder mode:
   - `Fast Apple Hardware`: uses `h264_videotoolbox`; best for speed on Apple Silicon.
   - `Smaller CPU x264`: uses `libx264`; slower, but usually better size/quality control.
8. Start the download and watch progress/logs.
9. Review the result screen.

---

## Output Files

Downloads are saved to:

```text
~/Downloads/youtube_downloads
```

When QuickTime conversion is enabled, the app keeps the downloaded file and creates an additional converted file:

```text
original-title.webm
fixed-original-title.mp4
```

The converted file uses H.264 video and AAC audio in an `.mp4` container, which is the safest default for QuickTime Player on macOS.

`Fast Apple Hardware` uses a bitrate-based VideoToolbox encode. `Smaller CPU x264` uses CRF-based x264 encoding and usually produces more size-efficient files, but it takes longer.

The exact original extension depends on the selected format and what `yt-dlp` can obtain from YouTube.

---

## How Downloading Works

The TUI delegates the actual media work to proven command line tools:

- `yt-dlp` handles video and playlist downloads.
- `ffmpeg` handles QuickTime-friendly H.264/AAC conversion.
- `h264_videotoolbox` is used for fast hardware encoding when `Fast Apple Hardware` is selected.
- `libx264` is used when `Smaller CPU x264` is selected.

When `Source Format` is `Auto best`, the app asks `yt-dlp` for the best available video and audio combination. For fixed resolutions like `1080`, `720`, or `480`, it asks for the best available stream at or below that height.

When a loaded source format is selected, the app uses that exact video format ID. If the selected source format is video-only, the app combines it with `bestaudio`. If the selected combined format is unavailable at download time, the selector falls back to `best`.

Loaded source formats are scoped to the current single URL lookup. If a source format is selected, the download runs in single-video mode. Leave `Source Format` on `Auto best` for playlist downloads.

The selected output format is passed to `yt-dlp` as the merge/remux target format. If YouTube does not provide the exact stream combination directly, `yt-dlp` and `ffmpeg` may still need to merge or remux the result.

---

## Known Limitations

- The app currently uses a fixed output directory.
- Running downloads cannot be cancelled from inside the TUI yet.
- Available format loading is scoped to the current single URL; playlist-wide per-item format selection is not structured yet.
- Playlist items are shown through logs, not as a structured queue yet.
- Installers and prebuilt release binaries are not available yet.
- Linux and Windows are not primary targets yet.

---

## Roadmap

Useful next improvements:

- Dependency check screen for missing `yt-dlp` or `ffmpeg`
- Configurable output directory
- Cancel/retry support
- Structured playlist queue
- Dedicated format list view with search/filtering
- GitHub Actions release builds for Apple Silicon and Intel Macs
- Homebrew formula for easier installation
- macOS dependency check screen with Homebrew install hints

---

## Legacy Zsh Script

You can still run the original script directly:

```bash
./yt-download-zsh-script.sh
```

The Zsh script is simpler and does not provide the Ratatui interface. It is kept for users who prefer the original prompt-based workflow.

---

## Tips

- If QuickTime still cannot open a downloaded file, enable QuickTime conversion and use the generated `fixed-*.mp4` file.
- VLC Player is recommended for playing any format directly without conversion.
- If `cargo run` fails because Rust is missing, install it with `brew install rust`.
- If downloading fails immediately, check that `yt-dlp` is installed and up to date.

---

## License

This project is licensed under the [MIT License](LICENSE).
