"""Claude Code PreToolUse hook adapter for efficient-ai.

Capabilities:
1. Rewrites Bash commands (`updatedInput.command`) to use compact H8 flags (`git status -sb`,
   `git log --oneline -n 15`, `git --no-pager diff`, `pytest -q --tb=short`, `cargo test -q`)
   or replaces whole-file `cat <large_file>` (>300 lines) with `outline <large_file>`.
2. Advises on unranged `Read` calls targeting large source files (>300 lines) via `systemMessage`.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


def rewrite_shell_cmd(cmd: str) -> str:
    t = cmd.strip()
    if not t or "|" in t or "&&" in t or ";" in t:
        return cmd

    parts = t.split()
    if not parts:
        return cmd

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
                    json.dump(
                        {
                            "systemMessage": (
                                f"[efficient-ai] Note: {p.name} has {line_count} lines. "
                                "Prefer ranged Read (offset/limit) or `outline <file>` for large files."
                            )
                        },
                        sys.stdout,
                    )
                    return 0

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
