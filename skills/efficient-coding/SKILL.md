---
name: efficient-coding
description: Cut session token usage via Rust `outline` reads, `squeeze` command wrapping, parallel calls, and prose-only brevity.
---

# Efficient Coding Workflow (`efficient-ai`)

Apply these rules to minimize context growth and API cost without losing code or reasoning quality:

## 1. Structure-Aware Reads with `outline` (Rust — Saves ~47% Read Tokens)
- **Batch independent calls** in a single turn (each saved early turn avoids re-sending context ~54×).
- **Outline before whole-file reads:** For files >200 lines, run `outline <file>` to inspect `[Lstart-Lend]` symbol spans before loading the entire file.
- **Target single symbols:** Read only the needed function/method via `outline <file> --symbol <name>` or `outline <file> --lines <start>-<end>`.
- **Search with enclosing symbols:** Use `outline search "<pattern>" [dir]` to find matches and their enclosing function/class in one step.

## 2. Lossless Command Compression with `squeeze` (Rust — Saves ~92% Stream Tokens)
- Wrap noisy test/build commands with `squeeze -- <cmd>` (e.g., `squeeze -- pytest`, `squeeze -- cargo test`, `squeeze -- npm test`).
- `squeeze` keeps 100% of errors, tracebacks, and warnings, elides passing noise, detects 3× stuck loops, and spills the raw output to `.eai/logs/last.log` if you need to inspect it.
- Prefer compact flags: `git status -sb`, `git log --oneline -n 15`, `git --no-pager diff -U1`.

## 3. Prose-Only Brevity (Saves ~65% Reply Prose)
- Skip preambles ("Let me...", "I will now...") and post-edit tool recaps.
- Never echo unchanged code or repeat diffs in prose; cite `path:line` instead.
