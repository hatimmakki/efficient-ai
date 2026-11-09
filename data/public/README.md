# Public Sanitized Datasets (`data/public/`)

This directory contains **privacy-sanitized, aggregated session metrics** exported by [`tokscope sanitize`](../../tools/tokscope/).

## Privacy Guarantee

- **Included:** 12-character SHA-256 session ID hashes, agent type (`claude`, `codex`, `gemini`), `is_sidechain` boolean, turn counts, provider usage totals (`uncached_input`, `cache_write`, `cache_write_5m`, `cache_write_1h`, `cache_read`, `output`, `reasoning`), cache hit rate, cost-weighted tokens, and allowlisted tool-category token totals.
- **Excluded (never exported):** File paths, directory names, usernames, prompts, assistant text, code snippets, command arguments, or environment variables.

## Files

- [`claude_sessions_summary.json`](claude_sessions_summary.json) — 315 real Claude Code transcripts (66 parent sessions + 249 sub-agent runs, 13,370 deduplicated API turns, 2.59B total raw tokens).
