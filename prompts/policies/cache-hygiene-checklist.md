# Prompt-Cache Hygiene Checklist

Because cached input reads cost **0.1×** (Anthropic) to **0.25×–0.5×** (Gemini/OpenAI) of base input price—and any byte change in the prefix invalidates everything after it:

1. **No dynamic values in static instruction files.** Never inject wall-clock timestamps, `git status` snapshots, random UUIDs, or environment variables into `AGENTS.md`, `CLAUDE.md`, or `GEMINI.md`.
2. **Deterministic tool & skill ordering.** Sort MCP tool definitions and skill metadata alphabetically at load time.
3. **Batch history compactions.** Never prune or rewrite earlier conversation turns on every turn; compact only at fixed turn intervals (e.g., every $K=10$–$15$ turns) so the prefix stays cache-stable between compactions.
4. **Keep ephemeral hints at the tail.** If a hook injects a warning or loop nudge (`additionalContext` / `systemMessage`), append it to the latest tool result rather than mutating the system prompt.
