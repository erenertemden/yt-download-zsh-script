#!/bin/zsh

set -e
set -u
set -o pipefail

for command_name in yt-dlp ffmpeg; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Missing required command: $command_name" >&2
    exit 1
  fi
done

echo "Enter the YouTube URL (single video or playlist):"
read url

if [[ -z "${url//[[:space:]]/}" ]]; then
  echo "URL is required." >&2
  exit 1
fi

echo "Which resolution do you want? (e.g., 1080, 720, 480):"
read resolution

if [[ ! "$resolution" == <-> ]]; then
  echo "Resolution must be a number, for example 1080." >&2
  exit 1
fi

echo "Which file format do you want? (mp4, webm, mkv):"
read format

case "$format" in
  mp4|webm|mkv) ;;
  *)
    echo "Format must be one of: mp4, webm, mkv" >&2
    exit 1
    ;;
esac

output_dir=~/Downloads/youtube_downloads
mkdir -p "$output_dir"

echo "Starting download to ${output_dir}..."

yt-dlp --newline --progress --yes-playlist \
  --merge-output-format "$format" \
  --remux-video "$format" \
  -f "bestvideo[height<=${resolution}]+bestaudio/best[height<=${resolution}]" \
  -o "${output_dir}/%(title)s [%(id)s].%(ext)s" "$url"

echo "Download completed."
echo "Converting downloaded ${format} files for QuickTime compatibility..."

cd "$output_dir"
files=(*.${format}(N))

if (( ${#files} == 0 )); then
  echo "No .${format} files were found to convert." >&2
  exit 1
fi

for file in "${files[@]}"; do
  output="fixed-${file:r}.mp4"
  echo "Converting: $file -> $output"
  ffmpeg -y -i "$file" \
    -c:v libx264 -preset fast -crf 23 -pix_fmt yuv420p \
    -c:a aac -b:a 128k -movflags +faststart \
    "$output"
done

echo "All videos were converted and saved in: $output_dir"
