---
name: tokscope
description: Profile agent session token economics, prompt-cache hit rates, and static-prefix overhead (`AGENTS.md`, skills, MCP).
---

# `tokscope` — Session Token & Static-Prefix Profiler

Use this skill when the user asks to analyze where their agent tokens or API costs go, or to audit their `AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, skills, or MCP configuration.

## Commands

```bash
# 1. Analyze full-session token economics, cache hit rate, & history amplification
tokscope analyze

# 2. Audit static-prefix token overhead in the project and user home directory
tokscope audit . --include-user

# 3. Export privacy-sanitized aggregate session metrics
tokscope sanitize --out data/public/sessions.json
```

## Privacy Rule
Never print or commit raw transcript lines; `tokscope` outputs aggregate counts only.
