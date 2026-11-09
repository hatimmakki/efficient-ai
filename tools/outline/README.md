# `outline` — Structure-Aware File Reader & Enclosing-Symbol Search

File reading accounts for **~64% of tool-output tokens** in real coding agent sessions ([Experiment 002](../../experiments/002-tool-output-attribution/RESULTS.md), [Experiment 003](../../experiments/003-session-token-economics/RESULTS.md)), and the largest 10% of reads consume 41.2% of all read tokens.

`outline` is a fast, zero-dependency Rust CLI that lets agents inspect file structure, read specific symbols or line ranges, and resolve search matches directly to their enclosing symbol—working portably across **Claude Code**, **Codex CLI**, and **Gemini CLI**.

## Build

```bash
cargo build --release --manifest-path tools/outline/Cargo.toml
```

## Usage

```bash
# 1. Structural symbol outline with [Lstart-Lend] spans (skips bodies & docstrings)
outline path/to/file.py

# 2. Targeted symbol read (prints full implementation of matching function/class/method)
outline path/to/file.py --symbol parse_claude_jsonl

# 3. Line-range read
outline path/to/file.py --lines 120-185

# 4. Search with enclosing symbol resolution (eliminates search-then-whole-file-read)
outline search "cache_write" tools/
outline search "cache_write" tools/ --expand   # Include full enclosing function body
```
