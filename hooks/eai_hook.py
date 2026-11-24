"""Unified zero-dependency hook dispatcher for Claude Code, Antigravity, and Gemini CLI.

Usage:
  python hooks/eai_hook.py <claude|antigravity|gemini>

If the compiled Rust `eai` binary is installed (on PATH, ~/.eai/bin, ~/.cargo/bin, or
repo target/release), delegates immediately to `eai hook <agent>` (<10ms).
Otherwise runs a safe, bug-free pure-Python fallback.
"""

from __future__ import annotations

import json
import os
import re
import shlex
import shutil
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent


def find_bin(name: str) -> str | None:
    exe = f"{name}.exe" if sys.platform == "win32" else name
    candidates = [
        REPO_ROOT / "target" / "release" / exe,
        REPO_ROOT / "tools" / name / "target" / "release" / exe,
        Path.home() / ".eai" / "bin" / exe,
        Path.home() / ".cargo" / "bin" / exe,
    ]
    for c in candidates:
        if c.is_file():
            return str(c)
    return shutil.which(name)


def _quote(arg: str) -> str:
    if not any(ch in arg for ch in (' ', '\t', '"', "'")):
        return arg
    return '"' + arg.replace('"', '\\"') + '"'


def _split_cmd(cmd: str) -> list[str]:
    try:
        return shlex.split(cmd, posix=(sys.platform != "win32"))
    except ValueError:
        return cmd.split()


def rewrite_shell_cmd(cmd: str, cwd: Path | None = None) -> str:
    t = cmd.strip()
    if (
        not t
        or "|" in t
        or "&&" in t
        or ";" in t
        or "\n" in t
        or t.startswith("squeeze ")
        or t.startswith("eai ")
    ):
        return cmd

    parts = _split_cmd(t)
    if not parts:
        return cmd

    base_cwd = cwd if cwd is not None else Path.cwd()
    eai_bin = find_bin("eai")
    outline_bin = find_bin("outline")
    squeeze_bin = find_bin("squeeze")

    # 1. Redirect `cat <large_file>` (>300 lines) to resolved `eai outline` or `outline` binary
    if parts[0] == "cat" and len(parts) == 2 and not parts[1].startswith("-"):
        raw_p = Path(parts[1])
        full_p = raw_p if raw_p.is_absolute() else (base_cwd / raw_p)
        if full_p.is_file():
            try:
                line_count = sum(1 for _ in full_p.open(encoding="utf-8", errors="ignore"))
                if line_count > 300:
                    if eai_bin:
                        return f"{_quote(eai_bin)} outline {_quote(parts[1])}"
                    if outline_bin:
                        return f"{_quote(outline_bin)} {_quote(parts[1])}"
            except OSError:
                pass

    # 2. Wrap noisy test/build suites with resolved `eai squeeze --` or `squeeze --`
    is_test_runner = (
        parts[0] in ("pytest", "vitest", "jest")
        or (parts[0] == "cargo" and len(parts) > 1 and parts[1] in ("test", "build", "check", "clippy"))
        or (parts[0] in ("npm", "pnpm", "yarn", "bun") and len(parts) > 1 and parts[1] == "test")
        or (parts[0] == "go" and len(parts) > 1 and parts[1] in ("test", "build"))
        or (parts[0] in ("python", "python3", "py") and parts[1:3] in (["-m", "pytest"], ["-m", "unittest"]))
        or (parts[0] == "uv" and parts[1:3] == ["run", "pytest"])
    )
    if is_test_runner:
        if eai_bin:
            return f"{_quote(eai_bin)} squeeze -- {t}"
        if squeeze_bin:
            return f"{_quote(squeeze_bin)} -- {t}"

    # 3. Compact H8 CLI flags
    if parts[0] == "git" and len(parts) >= 2:
        sub = parts[1]
        if sub == "status" and not any(p in ("-s", "-sb", "--short", "--porcelain") for p in parts[2:]):
            return f"{t} -sb"
        if sub == "log":
            out = t
            if not any(p.startswith(("--oneline", "--format", "--pretty")) for p in parts[2:]):
                out += " --oneline"
            if not any(p == "-n" or (p.startswith("-") and len(p) > 1 and p[1:].isdigit()) for p in parts[2:]):
                out += " -n 15"
            return out
        if sub == "diff" and "--no-pager" not in parts:
            return re.sub(r"^git\s+diff\b", "git --no-pager diff", t)

    if parts[0] == "pytest" and not any(p in ("-q", "--quiet", "-v", "-vv") for p in parts[1:]):
        return f"{t} -q --tb=short"

    if parts[:2] in (["cargo", "test"], ["cargo", "build"], ["cargo", "check"]) and not any(
        p in ("-q", "--quiet", "--") for p in parts[2:]
    ):
        return f"{t} -q"

    return cmd


def _handle_claude(payload: dict) -> dict | None:
    tool_name = str(payload.get("tool_name") or "")
    tool_input = payload.get("tool_input") or {}
    if not isinstance(tool_input, dict):
        return None
    cwd = Path(str(payload["cwd"])) if payload.get("cwd") else Path.cwd()

    if tool_name in ("Bash", "PowerShell"):
        orig_cmd = str(tool_input.get("command") or "")
        new_cmd = rewrite_shell_cmd(orig_cmd, cwd=cwd)
        if new_cmd != orig_cmd:
            updated = dict(tool_input)
            updated["command"] = new_cmd
            return {
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "allow",
                    "updatedInput": updated,
                }
            }
        return None

    if tool_name == "Read":
        fpath = tool_input.get("file_path")
        has_range = bool(tool_input.get("offset") or tool_input.get("limit"))
        if fpath and not has_range:
            raw_p = Path(str(fpath))
            full_p = raw_p if raw_p.is_absolute() else (cwd / raw_p)
            if full_p.is_file():
                try:
                    line_count = sum(1 for _ in full_p.open(encoding="utf-8", errors="ignore"))
                except OSError:
                    line_count = 0
                if line_count > 350:
                    # Ultra-compact gentle nudge (~18 tok), never dumps outline on top of full read
                    return {
                        "systemMessage": (
                            f"[eai] {full_p.name} is {line_count} lines; "
                            f"prefer `eai outline {full_p.name}` or ranged Read (offset/limit) next time."
                        )
                    }
    return None


def _handle_antigravity(payload: dict) -> dict:
    event = str(payload.get("hookEventName") or "PreToolUse")
    cwd = Path(str(payload["cwd"])) if payload.get("cwd") else Path.cwd()

    if event == "PreToolUse":
        tc = payload.get("toolCall") or {}
        if isinstance(tc, dict) and tc.get("name") == "run_command":
            args = tc.get("args") or {}
            if isinstance(args, dict):
                orig_cmd = str(args.get("CommandLine") or "")
                new_cmd = rewrite_shell_cmd(orig_cmd, cwd=cwd)
                if new_cmd != orig_cmd:
                    return {"decision": "allow", "overwrite": {"CommandLine": new_cmd}}
        return {"decision": "allow"}
    if event == "PostToolUse":
        return {}
    if event in ("PreInvocation", "PostInvocation"):
        return {"injectSteps": []}
    if event == "Stop":
        return {"decision": "allow"}
    return {}


def _handle_gemini(payload: dict) -> dict:
    tool_name = str(payload.get("tool_name") or "")
    tool_input = payload.get("tool_input") or {}
    cwd = Path(str(payload["cwd"])) if payload.get("cwd") else Path.cwd()

    if isinstance(tool_input, dict) and tool_name in ("run_shell_command", "shell", "bash"):
        orig_cmd = str(tool_input.get("command") or "")
        new_cmd = rewrite_shell_cmd(orig_cmd, cwd=cwd)
        if new_cmd != orig_cmd:
            updated = dict(tool_input)
            updated["command"] = new_cmd
            return {"decision": "allow", "tool_input": updated}
    return {"decision": "allow"}


def run_hook(agent: str) -> int:
    raw = sys.stdin.read()
    if not raw.strip():
        return 0

    # Fast path: delegate to compiled Rust `eai hook <agent>` (<10ms) if installed
    eai_bin = find_bin("eai")
    if eai_bin and os.environ.get("EAI_FORCE_PY_HOOK") != "1":
        try:
            res = subprocess.run(
                [eai_bin, "hook", agent],
                input=raw,
                text=True,
                encoding="utf-8",
                capture_output=True,
                timeout=4,
            )
            if res.stdout:
                sys.stdout.write(res.stdout)
            return res.returncode
        except Exception:
            pass

    try:
        payload = json.loads(raw)
    except json.JSONDecodeError:
        return 0

    if not isinstance(payload, dict):
        return 0

    if agent == "claude":
        out = _handle_claude(payload)
        if out is not None:
            json.dump(out, sys.stdout)
    elif agent == "antigravity":
        json.dump(_handle_antigravity(payload), sys.stdout)
    elif agent == "gemini":
        json.dump(_handle_gemini(payload), sys.stdout)
    return 0


def main() -> int:
    agent = sys.argv[1] if len(sys.argv) > 1 else "claude"
    return run_hook(agent)


if __name__ == "__main__":
    raise SystemExit(main())
