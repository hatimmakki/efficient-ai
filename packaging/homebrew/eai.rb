class Eai < Formula
  desc "Same quality, fewer tokens: unified CLI & local monitor for AI coding agents"
  homepage "https://github.com/hatimmakki/efficient-ai"
  version "1.0.0"
  license "Apache-2.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/hatimmakki/efficient-ai/releases/download/v1.0.0/eai-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_SHA256_FROM_RELEASE"
    else
      url "https://github.com/hatimmakki/efficient-ai/releases/download/v1.0.0/eai-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_SHA256_FROM_RELEASE"
    end
  end

  on_linux do
    url "https://github.com/hatimmakki/efficient-ai/releases/download/v1.0.0/eai-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "REPLACE_WITH_SHA256_FROM_RELEASE"
  end

  def install
    bin.install "eai"
  end

  test do
    assert_match "ALL 4 AGENT FIXTURES PASSED", shell_output("#{bin}/eai selftest")
  end
end
