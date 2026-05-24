class YtDownloadTui < Formula
  desc "macOS-first terminal UI for yt-dlp downloads"
  homepage "https://github.com/erenertemden/yt-download-zsh-script"
  license "MIT"
  head "https://github.com/erenertemden/yt-download-zsh-script.git", branch: "main"

  depends_on "rust" => :build
  depends_on "ffmpeg"
  depends_on "yt-dlp"

  def install
    system "cargo", "install", *std_cargo_args(path: ".")
  end

  test do
    assert_path_exists bin/"yt-download-tui"
  end
end
