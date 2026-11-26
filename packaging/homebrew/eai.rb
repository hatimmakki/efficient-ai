class Eai < Formula
  desc "Same quality, fewer tokens: unified CLI & local monitor for AI coding agents"
  homepage "https://github.com/hatimmakki/efficient-ai"
  version "1.2.0"
  license "Apache-2.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/hatimmakki/efficient-ai/releases/download/v1.2.0/eai-aarch64-apple-darwin.tar.gz"
      sha256 "c107daf11ee10235d300fe8ef769e7e261c6d24a4a762e427977bf6a9e7d61cc"
    else
      url "https://github.com/hatimmakki/efficient-ai/releases/download/v1.2.0/eai-x86_64-apple-darwin.tar.gz"
      sha256 "4ef110bd967ee0a977edd72186179bd2ab4d199cec71ad4e88f8625e278efecc"
    end
  end

  on_linux do
    url "https://github.com/hatimmakki/efficient-ai/releases/download/v1.2.0/eai-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "2a0a115d01161c0bec6a8129b2f1b7b2f0c1ce083923c01d6c8740c3ee7c243d"
  end

  def install
    bin.install "eai"
  end

  test do
    assert_match "ALL 4 AGENT FIXTURES PASSED", shell_output("#{bin}/eai selftest")
  end
end
