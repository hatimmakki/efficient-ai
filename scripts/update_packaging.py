#!/usr/bin/env python3
"""Stamp real SHA-256 hashes from a release SHA256SUMS.txt into Homebrew (`eai.rb`) & Scoop (`eai.json`).

Usage:
  python scripts/update_packaging.py --version 1.1.0 --sums artifacts/SHA256SUMS.txt --out-dir artifacts/
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent


def parse_sums(path: Path) -> dict[str, str]:
    sums: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        parts = line.strip().split()
        if len(parts) >= 2:
            digest = parts[0].lower()
            fname = Path(parts[-1].lstrip("*")).name
            sums[fname] = digest
    return sums


def render_homebrew(version: str, sums: dict[str, str]) -> str:
    arm64 = sums.get("eai-aarch64-apple-darwin.tar.gz", "REPLACE_WITH_SHA256")
    x64_mac = sums.get("eai-x86_64-apple-darwin.tar.gz", "REPLACE_WITH_SHA256")
    x64_lin = sums.get("eai-x86_64-unknown-linux-gnu.tar.gz", "REPLACE_WITH_SHA256")
    return f'''class Eai < Formula
  desc "Same quality, fewer tokens: unified CLI & local monitor for AI coding agents"
  homepage "https://github.com/hatimmakki/efficient-ai"
  version "{version}"
  license "Apache-2.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/hatimmakki/efficient-ai/releases/download/v{version}/eai-aarch64-apple-darwin.tar.gz"
      sha256 "{arm64}"
    else
      url "https://github.com/hatimmakki/efficient-ai/releases/download/v{version}/eai-x86_64-apple-darwin.tar.gz"
      sha256 "{x64_mac}"
    end
  end

  on_linux do
    url "https://github.com/hatimmakki/efficient-ai/releases/download/v{version}/eai-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "{x64_lin}"
  end

  def install
    bin.install "eai"
  end

  test do
    assert_match "ALL 4 AGENT FIXTURES PASSED", shell_output("#{{bin}}/eai selftest")
  end
end
'''


def render_scoop(version: str, sums: dict[str, str]) -> str:
    win_hash = sums.get("eai-x86_64-pc-windows-msvc.zip", "REPLACE_WITH_SHA256")
    manifest = {
        "version": version,
        "description": "Same quality, fewer tokens: unified CLI & local monitor for Claude Code, Antigravity, Gemini CLI, Codex & Cursor.",
        "homepage": "https://github.com/hatimmakki/efficient-ai",
        "license": "Apache-2.0",
        "architecture": {
            "64bit": {
                "url": f"https://github.com/hatimmakki/efficient-ai/releases/download/v{version}/eai-x86_64-pc-windows-msvc.zip",
                "hash": win_hash,
            }
        },
        "bin": "eai.exe",
        "checkver": "github",
        "autoupdate": {
            "architecture": {
                "64bit": {
                    "url": "https://github.com/hatimmakki/efficient-ai/releases/download/v$version/eai-x86_64-pc-windows-msvc.zip"
                }
            }
        },
    }
    return json.dumps(manifest, indent=2) + "\n"


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--version", required=True, help="Release version without leading v (e.g. 1.1.0)")
    p.add_argument("--sums", required=True, help="Path to SHA256SUMS.txt")
    p.add_argument("--out-dir", default=None, help="Optional output directory (defaults to updating packaging/ in place)")
    args = p.parse_args()

    ver = args.version.lstrip("v")
    sums = parse_sums(Path(args.sums))
    rb = render_homebrew(ver, sums)
    scoop = render_scoop(ver, sums)

    if args.out_dir:
        od = Path(args.out_dir)
        od.mkdir(parents=True, exist_ok=True)
        (od / "eai.rb").write_text(rb, encoding="utf-8")
        (od / "eai.json").write_text(scoop, encoding="utf-8")
        print(f"Wrote stamped eai.rb and eai.json to {od}")
    else:
        (REPO_ROOT / "packaging" / "homebrew" / "eai.rb").write_text(rb, encoding="utf-8")
        (REPO_ROOT / "packaging" / "scoop" / "eai.json").write_text(scoop, encoding="utf-8")
        print("Updated packaging/homebrew/eai.rb and packaging/scoop/eai.json in place")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
