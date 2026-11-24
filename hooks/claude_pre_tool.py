"""Claude Code PreToolUse hook adapter (delegates to unified hooks/eai_hook.py)."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from eai_hook import find_bin, rewrite_shell_cmd, run_hook  # noqa: E402,F401


def main() -> int:
    return run_hook("claude")


if __name__ == "__main__":
    raise SystemExit(main())
