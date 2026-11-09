# `prompts/` — Token-Budgeted Agent Policies

Every instruction in `AGENTS.md` is paid on **every turn** of a session. These policy files are designed for maximum behavioral leverage per token and portable single-source sharing across **Claude Code**, **Codex CLI**, and **Gemini CLI**.

## Single-Source Setup

1. Copy [`AGENTS.md`](AGENTS.md) (**227 tokens** on `o200k_base`) to your repository root.
2. For **Claude Code**, copy [`CLAUDE.md`](CLAUDE.md) (`@AGENTS.md`, **3 tokens**) to your repository root.
3. For **Gemini CLI**, merge [`GEMINI.settings.json`](GEMINI.settings.json) (`"contextFileName": ["AGENTS.md"]`) into `.gemini/settings.json`.
4. **Codex CLI** reads `AGENTS.md` natively.

## Modular Policy Snippets (`policies/`)

Each snippet can be A/B tested independently:

| Policy Snippet | Hypothesis | `o200k_base` Tokens | 50-Turn Cached Cost (`1×` eq.) |
|---|---|---:|---:|
| [`AGENTS.md`](AGENTS.md) (Combined bundle) | H3 + H4 + H6 + H8 | 227 | 1,396 |
| [`policies/h3-turn-and-read-discipline.md`](policies/h3-turn-and-read-discipline.md) | H3, H4 (Turn & read discipline) | 71 | 437 |
| [`policies/h6-prose-brevity.md`](policies/h6-prose-brevity.md) | H6 (Prose-only brevity) | 60 | 369 |
| [`policies/h8-compact-cli-flags.md`](policies/h8-compact-cli-flags.md) | H8 (Machine-friendly CLI flags) | 56 | 344 |
| [`policies/cache-hygiene-checklist.md`](policies/cache-hygiene-checklist.md) | Prompt-cache hygiene reference (not loaded in prompt) | — | 0 |
