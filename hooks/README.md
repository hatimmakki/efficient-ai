# `hooks/` — Portable Agent Interception Adapters

As documented in [`docs/research/01-agent-surfaces.md`](../docs/research/01-agent-surfaces.md):
- **Claude Code** supports `PreToolUse` argument rewriting (`updatedInput`) for `Bash` and `Read`.
- **Gemini CLI** supports `BeforeTool` argument rewriting (`tool_input`) and `AfterTool` replacement.
- **Codex CLI** rejects `updatedInput` in `PreToolUse` hooks; intercepting commands via `deny` costs an extra turn (triggering a recovery loop).

Therefore, we provide **two interception layers**:
1. **Primary (All 3 Agents, including Codex CLI): Portable `PATH` Shims** via `squeeze --install-shims .eai/shims`. Prepending `.eai/shims` to `PATH` wraps `pytest`, `cargo`, and `npm` transparently with zero extra turns.
2. **Secondary (Claude Code & Gemini CLI): Pre-Tool Rewrite Hooks** (`claude_pre_tool.py` and `gemini_before_tool.py`) that inject compact **H8** flags (`git status -sb`, `git log --oneline -n 15`, `git --no-pager diff`, `pytest -q --tb=short`) and nudge on whole-file reads $>350$ lines.

## Setup

### 1. Portable `PATH` Shims (Claude Code, Codex CLI, Gemini CLI)
```bash
cargo build --release --manifest-path tools/squeeze/Cargo.toml
./tools/squeeze/target/release/squeeze --install-shims .eai/shims
export PATH="$PWD/.eai/shims:$PATH"   # PowerShell: $env:PATH = "$PWD\.eai\shims;$env:PATH"
```

### 2. Claude Code Hook (`.claude/settings.json`)
Merge [`claude.settings.json`](claude.settings.json) into `.claude/settings.json`.

### 3. Gemini CLI Hook (`.gemini/settings.json`)
Merge [`gemini.settings.json`](gemini.settings.json) into `.gemini/settings.json`.
