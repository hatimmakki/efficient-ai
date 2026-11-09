---
name: efficient-coding
description: Cut session token usage via outline-first reads, parallel calls, compact CLI flags, and prose-only brevity.
---

# Efficient Coding Workflow (`efficient-ai`)

Apply these rules to minimize context growth and API cost without losing code or reasoning quality:

## 1. Read & Turn Discipline (Saves ~47% Read Tokens)
- **Batch independent calls** in a single turn (each saved early turn avoids re-sending context ~54×).
- **Outline before whole-file reads:** For files >200 lines, inspect signatures (`outline <file>` or ranged `Read`) before loading the entire file.
- **Target enclosing symbols after search:** Read only the matched function/class (`outline <file> --symbol <name>` or `Read` with `offset`/`limit`), not the whole file.
- **Stop when verified:** Once tests/checks pass, stop immediately without redundant re-runs.

## 2. Prose-Only Brevity (Saves ~65% Reply Prose)
- Skip preambles ("Let me...", "I will now...") and post-edit tool recaps.
- Never echo unchanged code or repeat diffs in prose; cite `path:line` instead.
- Keep 100% of detail in code, tests, diffs, and reasoning.

## 3. Compact CLI Flags (Saves 67–97% Stream Tokens)
- Prefer terse, non-interactive flags: `git status -sb`, `git log --oneline -n 15`, `git --no-pager diff -U1`, `pytest -q --tb=short`, `cargo test -q`.
