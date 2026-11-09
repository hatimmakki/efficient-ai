"""Claude Code PreToolUse hook adapter for efficient-ai.

Wires Claude Code directly to the Rust `squeeze` and `outline` binaries when available:
1. Rewrites Bash commands (`updatedInput.command`):
   - Uses `squeeze --rewrite` (or built-in fallback) for compact H8 git flags (`git status -sb`, `git log --oneline -n 15`).
   - Wraps noisy test/build runs (`pytest`, `cargo test`, `npm test`) with the Rust `squeeze --` binary when installed,
     enabling H2 lossless `.eai/logs/last.log` spill compression and H7 3x stuck-loop detection.
   - Rewrites `cat <large_file>` (>300 lines) to `outline <large_file>` when the Rust `outline` binary is available.
2. When an unranged `Read` call targets a file >350 lines, runs the Rust `outline` binary and attaches a compact
   `[Lstart-Lend]` symbol preview in `systemMessage`.
"""

from __future__ import annotations

import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent


def find_rust_bin(name: str) -> str | None:
    exe = f"{name}.exe" if sys.platform == "win32" else name
    local_rel = REPO_ROOT / "tools" / name / "target" / "release" / exe
    if local_rel.is_file():
        return str(local_rel)
    cargo_bin = Path.home() / ".cargo" / "bin" / exe
    if cargo_bin.is_file():
        return str(cargo_bin)
    return shutil.which(name)


def rewrite_shell_cmd(cmd: str) -> str:
    t = cmd.strip()
    if not t or "|" in t or "&&" in t or ";" in t or t.startswith("squeeze "):
        return cmd

    parts = t.split()
    if not parts:
        return cmd

    outline_bin = find_rust_bin("outline")
    squeeze_bin = find_rust_bin("squeeze")

    # 1. If `cat <single_file>` is called on a source file >300 lines, redirect to `outline <file>`
    if parts[0] == "cat" and len(parts) == 2 and not parts[1].startswith("-") and outline_bin:
        p = Path(parts[1])
        if p.is_file():
            try:
                if sum(1 for _ in p.open(encoding="utf-8", errors="ignore")) > 300:
                    return f"outline {parts[1]}"
            except Exception:
                pass

    # 2. Wrap test suites with `squeeze --` when the Rust binary is available
    if squeeze_bin and (
        parts[0] == "pytest"
        or parts[:2] == ["cargo", "test"]
        or parts[:2] == ["npm", "test"]
    ):
        return f"squeeze -- {t}"

    # 3. Delegate to `squeeze --rewrite` if installed, else Python H8 rules
    if squeeze_bin:
        try:
            res = subprocess.run(
                [squeeze_bin, "--rewrite", t],
                capture_output=True,
                text=True,
                encoding="utf-8",
                timeout=2,
                check=True,
            )
            rewritten = res.stdout.strip()
            if rewritten:
                return rewritten
        except Exception:
            pass

    if parts[0] == "git" and len(parts) >= 2:
        sub = parts[1]
        if sub == "status" and not any(p in ("-s", "-sb", "--short", "--porcelain") for p in parts[2:]):
            return f"{t} -sb"
        if sub == "log":
            out = t
            if not any(p.startswith(("--oneline", "--format", "--pretty")) for p in parts[2:]):
                out += " --oneline"
            if not any(p == "-n" or (p.startswith("-") and p[1:].isdigit()) for p in parts[2:]):
                out += " -n 15"
            return out
        if sub == "diff" and "--no-pager" not in parts:
            return re.sub(r"^git\s+diff\b", "git --no-pager diff", t)

    if parts[0] == "pytest" and not any(p in ("-q", "--quiet", "-v", "-vv") for p in parts[1:]):
        return f"{t} -q --tb=short"

    if parts[:2] == ["cargo", "test"] and not any(p in ("-q", "--quiet", "--") for p in parts[2:]):
        return f"{t} -q"

    return cmd


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except Exception:
        return 0

    tool_name = payload.get("tool_name") or ""
    tool_input = payload.get("tool_input") or {}
    if not isinstance(tool_input, dict):
        return 0

    if tool_name in ("Bash", "PowerShell"):
        orig_cmd = str(tool_input.get("command") or "")
        new_cmd = rewrite_shell_cmd(orig_cmd)
        if new_cmd != orig_cmd:
            updated = dict(tool_input)
            updated["command"] = new_cmd
            json.dump(
                {
                    "hookSpecificOutput": {
                        "hookEventName": "PreToolUse",
                        "permissionDecision": "allow",
                        "updatedInput": updated,
                    }
                },
                sys.stdout,
            )
            return 0

    if tool_name == "Read":
        fpath = tool_input.get("file_path")
        has_range = bool(tool_input.get("offset") or tool_input.get("limit"))
        if fpath and not has_range:
            p = Path(str(fpath))
            if p.is_file():
                try:
                    line_count = sum(1 for _ in p.open(encoding="utf-8", errors="ignore"))
                except Exception:
                    line_count = 0
                if line_count > 350:
                    outline_bin = find_rust_bin("outline")
                    outline_preview = ""
                    if outline_bin:
                        try:
                            res = subprocess.run(
                                [outline_bin, str(p)],
                                capture_output=True,
                                text=True,
                                encoding="utf-8",
                                timeout=2,
                                check=True,
                            )
                            preview_lines = res.stdout.strip().splitlines()[:15]
                            outline_preview = "\n" + "\n".join(preview_lines)
                        except Exception:
                            pass
                    json.dump(
                        {
                            "systemMessage": (
                                f"[efficient-ai] {p.name} has {line_count} lines. "
                                f"Next time, run `outline {p.name}` or use ranged Read (offset/limit).{outline_preview}"
                            )
                        },
                        sys.stdout,
                    )
                    return 0

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
