# Agent Efficiency Policy

## Read & Turn Discipline (H3, H4)
- Batch independent tool calls in a single turn.
- For files >200 lines, inspect signatures (`outline <path>`) or targeted line ranges before whole-file reads.
- After search, read only the enclosing symbol or matched range, not the whole file.
- Stop immediately once verification passes; do not re-run passing checks.

## Output Brevity — Prose Only (H6)
- Keep prose direct and brief. Skip preambles ("Let me...", "I will now...") and post-change recaps.
- Never echo unchanged code or repeat diff contents in prose; cite `path:line` instead.
- Never sacrifice code completeness, test coverage, or reasoning depth for brevity.

## Compact Command Flags (H8)
- Pass non-interactive, terse flags to CLI tools: `git status -sb`, `git log --oneline -n 10`, `git --no-pager diff -U1`, `pytest -q --tb=short`, `cargo test -q`, `npm test --silent`.
