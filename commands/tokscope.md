---
description: Analyze session token economics, prompt-cache hit rates, and tool attribution with tokscope
---

Run `tokscope analyze` to profile the user's local agent transcripts (100% offline, aggregate counts only) and summarize where raw tokens and cost-weighted tokens go:

```bash
python "${CLAUDE_PLUGIN_ROOT:-.}/scripts/eai.py" analyze $ARGUMENTS
```
