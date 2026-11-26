---
name: eai
description: Control efficient-ai (`eai update`, `eai statusline`, `eai impact`, `eai pause`/`resume`, `eai doctor`).
---

# `/eai` Quick Controls

Run the matching `eai` subcommand and summarize the result concisely:

- **`/eai update`** (`eai update` / `eai update --check`): check GitHub Releases and self-update the `eai` binary (verifies SHA-256).
- **`/eai statusline`** (`eai statusline install` / `eai statusline off` / `eai statusline preview`): enable, disable, or preview the zero-token bottom telemetry bar in Antigravity CLI and Claude Code.
- **`/eai status`** (`eai doctor` + `eai stats`): inspect hooks, latency, statusline state, savings, and cached update status.
- **`/eai impact`** (`eai impact --project . --since 30d`): repo-scoped Before vs. After USD & cache-hit proof.
- **`/eai pause` / `/eai resume`**: toggle hook rewriting cleanly.
