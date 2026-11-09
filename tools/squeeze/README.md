# `squeeze` — Lossless Retrievable Output Compression & Loop Detection

Lossy stdout filters often fail end-to-end because stripping lines that the agent needed triggers **recovery loops** (extra turns that cost more than the filter saved).

`squeeze` implements **lossless-by-default retrievable compression (H2)**, **machine-friendly CLI flag rewriting (H8)**, and **stuck-loop detection (H7)**:

1. **Strips noise:** Removes ANSI escape codes, `\r` carriage-return spinner/progress bar overwrites, and collapses repeated identical lines with `[xN]`.
2. **Elides routine passing output, keeps 100% of errors:** Strips `PASSED` / `... ok` / `Downloading` / `Compiling` lines while keeping every failure, traceback, warning, assertion, and summary line intact.
3. **Spills full log + 1-line pointer:** Whenever lines are elided, writes the complete unmodified output to `.eai/logs/last.log` and prints:
   `[312 passing/duplicate lines elided (320 -> 8 lines); full log: .eai/logs/last.log]`
4. **Detects stuck loops (H7):** If the same failing command + error signature repeats $\ge 3$ times in a row, appends a `[LOOP DETECTED]` diagnostic nudge.

## Build

```bash
cargo build --release --manifest-path tools/squeeze/Cargo.toml
```

## Usage

```bash
# 1. Pipe any command output through squeeze
pytest -v | squeeze

# 2. Wrap a command directly (preserves exit code + runs H7 loop detector)
squeeze -- pytest -v

# 3. Rewrite a command with H8 compact flags
squeeze --rewrite "git status"          # -> git status -sb
squeeze --rewrite "git log"             # -> git log --oneline -n 15

# 4. Install portable PATH shims (works across Claude Code, Codex CLI, Gemini CLI)
squeeze --install-shims .eai/shims
```
