#!/bin/zsh

echo "Enter the YouTube URL (single video or playlist):"
read url

echo "Which resolution do you want? (e.g., 1080, 720, 480):"
read resolution

echo "Which file format do you want? (e.g., mp4, webm, mkv):"
read format

# Output directory
output_dir=~/Downloads/youtube_downloads
mkdir -p "$output_dir"

echo "Starting download to ${output_dir}..."

# Download videos with progress bar, playlist supported
yt-dlp --progress --yes-playlist \
  -f "bestvideo[ext=${format}][height<=${resolution}]+bestaudio[ext=m4a]/best[ext=${format}]" \
  -o "${output_dir}/%(title)s.%(ext)s" "$url"

echo "Download completed! ✅"

# Convert all downloaded files for QuickTime compatibility
echo "Converting videos for QuickTime compatibility..."

cd "$output_dir"

for file in *.${format}; do
  if [[ -f "$file" ]]; then
    echo "Converting: $file..."
    ffmpeg -i "$file" -c:v libx264 -preset fast -crf 23 -c:a aac -b:a 128k "fixed-$file"
  fi
done

echo "All videos are converted and saved in: $output_dir ✅🎉"

