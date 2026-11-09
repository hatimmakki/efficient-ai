# Agent Surfaces: Claude Code, Codex CLI, Gemini CLI

*Where each agent lets us measure and intervene. Compiled 2026-11-01.*

> ⚠️ Hook capabilities change fast and most of this comes from secondary sources. Every row marked ⚠️ must be verified against the agent's current docs or source **before** we build on it. `tools/tokscope` will include a self-test per agent.

## 1. Capability matrix

| Surface | Claude Code | Codex CLI | Gemini CLI |
|---|---|---|---|
| **Instruction file** | `CLAUDE.md` (can `@AGENTS.md` import; reads `AGENTS.md`) ⚠️ | `AGENTS.md` | `GEMINI.md`, configurable via `contextFileName` (e.g. `["AGENTS.md"]`) |
| **Skills (progressive disclosure)** | yes | partial / via AGENTS.md ⚠️ | yes |
| **Hooks config** | `.claude/settings.json` | `~/.codex/hooks.json` or `[hooks]` in `config.toml`; needs `[features] hooks = true` ⚠️ | `.gemini/settings.json` |
| **Pre-tool: block** | ✅ `PreToolUse` (exit 2 / `permissionDecision`) | ✅ `PreToolUse` | ✅ `BeforeTool` (`decision: deny`) |
| **Pre-tool: rewrite args** | ✅ `updatedInput` (whitelisted: Bash `command`, file paths) ⚠️ | ❌ `updatedInput` rejected by parser ⚠️ | ✅ `tool_input` rewrite ⚠️ |
| **Post-tool: replace output** | ⚠️ MCP tools only (`updatedMCPToolOutput`) | ❌ observe only ⚠️ | ✅ any tool: `decision: deny` + `reason` replaces the result ⚠️ |
| **Post-tool: append context** | `systemMessage` / additional context | `systemMessage` ⚠️ | `additionalContext` |
| **Tools covered by hooks** | all built-ins + MCP | mainly shell (+`apply_patch`); `read_file`/`grep` may bypass hooks ⚠️ | all |
| **Transcript location** | `~/.claude/projects/<encoded-cwd>/<session>.jsonl` | `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` | `~/.gemini/tmp/<project_hash>/chats/*.json` |
| **Usage fields** | `message.usage`: `input_tokens`, `cache_creation_input_tokens` (split 5m/1h), `cache_read_input_tokens`, `output_tokens`, `output_tokens_details.thinking_tokens` | `event_msg` → `token_count`: `total_token_usage` / `last_token_usage` with `input_tokens`, `cached_input_tokens`, `output_tokens`, `reasoning_output_tokens` | Gemini `usageMetadata`: `promptTokenCount`, `cachedContentTokenCount`, `candidatesTokenCount`, `thoughtsTokenCount` ⚠️ (also OpenTelemetry export) |
| **Accounting gotchas** | One API response is split into several JSONL lines (one per content block) sharing `message.id`/`requestId`: **earlier lines carry `message_start` placeholder `output_tokens` (`1..8`), while the final line carries the true `message_delta` `output_tokens`**. Dedupe by `message.id` and take the **last/max** usage record while merging `content` blocks. Also filter strictly to `type in ("assistant", "user")` to ignore `api-request-blob` wire echoes. Sub-agents live under `<session>/subagents/agent-*.jsonl` (`isSidechain: true`). | `total_token_usage` is cumulative: use `last_token_usage` deltas, and watch for duplicate emissions. `cached_input_tokens` ⊂ `input_tokens` (unlike Anthropic, where they're disjoint). | Format least documented: needs a probe on a real install. `cachedContentTokenCount` ⊂ `promptTokenCount`. |

🧪 **Verified locally (Claude Code, 2026-11-09):** Across 315 non-empty transcripts (66 parent sessions + 249 sub-agent runs, 13,370 deduplicated API turns), 13,788 duplicate streaming JSONL lines were merged. This resolves why older reports claimed `output_tokens` was a placeholder: keeping the first JSONL line per `message.id` reads the initial `message_start` placeholder (`1..8`), whereas keeping the final line reads the true `message_delta` count. Synthetic fixtures for all three agents are verified via `tokscope selftest` in [`tools/tokscope/fixtures/`](../../tools/tokscope/fixtures/).

## 2. Design consequences

1. **The only portable interception point is the command itself.** Codex can't rewrite tool args, and Claude Code can't replace built-in tool output. So the one mechanism that works on all three is **wrapping the shell command**:
   - **Primary: PATH shims.** A shim dir with `git`, `npm`, `pytest`, `cargo`, … that forwards to `squeeze` is prepended to `PATH` for agent sessions. Zero per-agent code, no extra turns, works even where hooks can't rewrite.
   - **Secondary: per-agent rewrite hooks.** Claude Code `updatedInput`, Gemini CLI `tool_input`. These cover commands we can't shim (pipelines, shell builtins).
   - ❌ **Avoid "deny + tell the model to use the wrapper"** (the only option on Codex via hooks). It costs an extra turn every time, which is exactly the recovery-loop failure mode from the literature review.
2. **Post-processing output is Gemini-only (plus Claude for MCP).** Treat it as a bonus adapter, not the core design.
3. **One instruction source.** Keep policy in `AGENTS.md`. Claude Code imports it (`@AGENTS.md`); Gemini CLI points `contextFileName` at it. Tool-specific files stay as thin shims. That way an A/B of a prompt policy is the *same text* across all three agents.
4. **Normalize usage into one schema.** `tokscope` maps each agent's fields onto:
   `uncached_input, cache_write, cache_read, output, reasoning`
   with per-provider rules (Anthropic disjoint vs OpenAI/Gemini "cached ⊂ input").
5. **Read tools differ.** Claude `Read`, Codex `read_file`, Gemini `read_file`/`read_many_files`. Outline-first reads (H4) must be offered as a **shell command** (`outline path`) to be portable, with optional MCP exposure.

## 3. Open verification tasks

- [ ] Install Codex CLI and Gemini CLI on the dev machine; record one short session each; snapshot the schema (keys only, no content) into `tools/tokscope/fixtures/` as **synthetic** fixtures.
- [ ] Confirm Codex `updatedInput` rejection and hook tool coverage in the current release.
- [ ] Confirm Gemini `AfterTool` deny+reason replaces output for built-in tools.
- [ ] Confirm whether Claude Code `PostToolUse` can replace built-in output in the current version.
- [ ] Check whether each agent honors a modified `PATH` from the launching shell (sandboxing may reset env; Codex `shell_environment_policy`).

