# tokscope

`tokscope` is the Phase 0 instrumentation and auditing tool for [`efficient-ai`](../../README.md).

It parses agent transcripts (**Claude Code**, **Codex CLI**, and **Gemini CLI**), normalizes provider usage fields into a single cost-aware schema, models **re-sent history amplification** across turns, and audits static-prefix token overhead (`AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, skills, and MCP configs).

## Commands

```bash
# Install into the project virtualenv
.venv/bin/pip install -e tools/tokscope      # Windows: .venv\Scripts\pip install -e tools/tokscope

# 1. Run schema & accounting self-tests on synthetic fixtures
tokscope selftest

# 2. Analyze full-session token economics and history amplification
tokscope analyze                             # Defaults to ~/.claude/projects
tokscope analyze path/to/rollout.jsonl --agent codex

# 3. Audit static prefix overhead in a project (and optionally user global config)
tokscope audit .
tokscope audit . --include-user

# 4. Export privacy-sanitized per-session aggregate JSON (safe for data/public/)
tokscope sanitize --out data/public/claude_sessions_summary.json
```
