class Eai < Formula
  desc "Same quality, fewer tokens: unified CLI & local monitor for AI coding agents"
  homepage "https://github.com/hatimmakki/efficient-ai"
  version "1.1.0"
  license "Apache-2.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/hatimmakki/efficient-ai/releases/download/v1.1.0/eai-aarch64-apple-darwin.tar.gz"
      sha256 "1d0665261b224f16c744da1082b65877d86b55c4c285667fbee5016122060477"
    else
      url "https://github.com/hatimmakki/efficient-ai/releases/download/v1.1.0/eai-x86_64-apple-darwin.tar.gz"
      sha256 "16cffb5dbdd79639e1877b9ba41e735d9732bbf0d52f659f641cb4fe53103930"
    end
  end

  on_linux do
    url "https://github.com/hatimmakki/efficient-ai/releases/download/v1.1.0/eai-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "b2a6a341c377c376464de540d01a19e3959cfcb188521daf322ee2d01cc3e3f9"
  end

  def install
    bin.install "eai"
  end

  test do
    assert_match "ALL 4 AGENT FIXTURES PASSED", shell_output("#{bin}/eai selftest")
  end
end
