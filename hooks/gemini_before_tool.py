"""Gemini CLI BeforeTool hook adapter for efficient-ai."""

from __future__ import annotations

import json
import sys

from claude_pre_tool import rewrite_shell_cmd


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except Exception:
        return 0

    tool_name = payload.get("tool_name") or ""
    tool_input = payload.get("tool_input") or {}
    if not isinstance(tool_input, dict):
        return 0

    if tool_name in ("run_shell_command", "shell", "bash"):
        orig_cmd = str(tool_input.get("command") or "")
        new_cmd = rewrite_shell_cmd(orig_cmd)
        if new_cmd != orig_cmd:
            updated = dict(tool_input)
            updated["command"] = new_cmd
            json.dump({"decision": "allow", "tool_input": updated}, sys.stdout)
            return 0

    json.dump({"decision": "allow"}, sys.stdout)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
