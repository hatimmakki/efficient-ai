# `hooks/` — Portable Multi-Agent Interception Adapters

As documented in [`docs/research/01-agent-surfaces.md`](../docs/research/01-agent-surfaces.md):
- **Claude Code** supports `PreToolUse` argument rewriting (`updatedInput`) for `Bash`, `PowerShell`, and `Read`.
- **Antigravity** supports camelCase `PreToolUse` `overwrite` on `run_command` (`CommandLine`) and `read_file` via root [`hooks.json`](../hooks.json) or `.agents/hooks.json`.
- **Gemini CLI** supports both `BeforeTool` argument rewriting (`tool_input`) and `AfterTool` lossless response compression.
- **Codex CLI** rejects `updatedInput` in `PreToolUse` hooks; therefore `eai init` installs executable `PATH` shims in `.eai/shims/` (zero extra turns).

## Automatic Setup (Recommended)

Run `eai init` (project) or `eai init --global` to wire native sub-10ms Rust hooks (`eai hook <agent>`) cleanly with automatic backups and idempotent JSON merging:

```bash
eai init            # Wires Claude Code, Antigravity, Gemini CLI & Codex shims for the current project
eai init --global   # Wires global configs (~/.claude, ~/.gemini/config, ~/.gemini, ~/.eai/shims)
eai doctor          # Verifies hook wiring & <10ms round-trip latency
eai uninstall       # Cleanly removes efficient-ai entries without touching your other hooks
```

## Architecture
- **Fast path (`<10ms`):** `eai hook <claude|antigravity|gemini>` runs inside the compiled Rust binary, rewriting commands using the exact resolved `eai` executable path (never failing with `command not found`), tracking `.eai/logs/` recovery rate, and keeping `Read` nudges compact (`~18` tokens, no double-tax).
- **Zero-dep marketplace fallback:** [`eai_hook.py`](eai_hook.py) delegates to `eai hook <agent>` whenever installed and otherwise falls back cleanly to pure Python 3.10+.
