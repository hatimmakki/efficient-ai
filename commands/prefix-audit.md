---
description: Audit static-prefix token overhead (AGENTS.md, CLAUDE.md, skills, MCP schemas) and suggest savings
---

Run `tokscope audit` on the current workspace and user global config, then highlight the largest per-turn token contributors and 50-turn session tax:

```bash
python "${CLAUDE_PLUGIN_ROOT:-.}/scripts/eai.py" audit . --include-user
```
