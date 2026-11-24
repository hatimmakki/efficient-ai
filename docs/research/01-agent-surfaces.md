# Agent Surfaces: Claude Code, Antigravity, Gemini CLI, Codex CLI & IDE/MCP Agents

*Where each agent lets us measure and intervene. Updated 2026-11-24 for `eai v1.0.0`.*

## 1. Verified Capability Matrix

| Surface | Claude Code | Antigravity (CLI / IDE / 2.0) | Gemini CLI | Codex CLI | Cursor / Windsurf / Cline / OpenCode |
|---|---|---|---|---|---|
| **Instruction file** | `CLAUDE.md` (`@AGENTS.md`) 🧪 | `AGENTS.md`, `GEMINI.md`, `rules/AGENTS.md` 🧪 | `GEMINI.md` / `contextFileName: ["AGENTS.md"]` | `AGENTS.md` | `.cursor/rules/efficient-ai.mdc`, `AGENTS.md` |
| **Native plugin manifest** | `.claude-plugin/plugin.json` + `marketplace.json` 🧪 | `.antigravity-plugin/plugin.json` + root `hooks.json` 🧪 | `gemini-extension.json` | `.codex-plugin/plugin.json` + `.agents/plugins/marketplace.json` | `eai mcp` + `npx skills add` |
| **Skills (progressive disclosure)** | ✅ `skills/*/SKILL.md` 🧪 | ✅ `skills/*/SKILL.md` 🧪 | ✅ `skills/*/SKILL.md` | ✅ `skills/*/SKILL.md` | ✅ via `npx skills add` |
| **Pre-tool: rewrite args** | ✅ `PreToolUse` `updatedInput` (`Bash`, `PowerShell`) 🧪 | ✅ `PreToolUse` `overwrite` (`run_command.CommandLine`) 🧪 | ✅ `BeforeTool` `tool_input` rewrite | ❌ `updatedInput` rejected -> **use `PATH` shims** | ✅ via `eai mcp` native tools |
| **Post-tool / Post-invocation** | `PostToolUse` / `systemMessage` | ✅ `PostInvocation` zero-LLM-token `metadata.user_markdown` cards 🧪 | ✅ `AfterTool` lossless stream compression | Observe only | MCP tool return directly compressed |
| **Transcript location** | `~/.claude/projects/**/*.jsonl` 🧪 | `~/.gemini/antigravity*/brain/**/.system_generated/logs/transcript.jsonl` 🧪 | `~/.gemini/tmp/**/chats/*.json` | `~/.codex/sessions/**/rollout-*.jsonl` | Local `.eai/metrics.jsonl` |
| **Token accounting model** | **Disjoint:** `input_tokens` + `cache_creation` (5m/1h) + `cache_read` 🧪 | **Disjoint:** `input_tokens` + `cache_read_tokens` (`PLANNER_RESPONSE` + 1:1 `GENERIC` tool outputs) 🧪 | **Subset:** `cachedContentTokenCount ⊂ promptTokenCount` | **Subset:** `cached_input_tokens ⊂ input_tokens` (cumulative deltas) | Direct local telemetry via `eai mcp` |

🧪 **Live-verified corpora:**
- **Claude Code:** 315 real transcripts (66 parent + 249 sub-agent runs, 13,370 deduplicated turns, 2.59B raw tokens; 13,788 split streaming lines merged).
- **Antigravity:** 2,509 real transcripts (49,553 API turns, 5.64B raw tokens, 93.3% prompt-cache hit rate; verified disjoint `input_tokens` + `cache_read_tokens` accounting and 1:1 `GENERIC` tool-result mapping).
- **Codex CLI & Gemini CLI:** Verified via synthetic fixtures in `eai selftest` and `tools/tokscope/fixtures/`.

## 2. Architecture Consequences (Implemented in `eai v1.0.0`)

1. **Single Sub-10ms Rust Binary (`eai`):** One binary replaces fragmented Python scripts across every agent (`eai hook claude|antigravity|gemini`, `eai mcp`, `eai outline`, `eai squeeze`, `eai proxy`, `eai dashboard`).
2. **Resolved Absolute Binary Paths in Hooks:** Hooks rewrite commands using `std::env::current_exe()` so subshells never fail with `command not found`.
3. **Portable Executable `PATH` Shims (Codex & Universal Fallback):** `eai init` writes `0o755` Unix shims + Windows `.cmd` shims in `.eai/shims/` wrapping `pytest`, `cargo`, `npm`, `pnpm`, `yarn`, and `go` with zero extra turns.
4. **Ultra-Lean Universal MCP (`eai mcp`):** Exposes 4 surgical tools (`outline_file`, `outline_search`, `squeeze_run`, `eai_stats`) in **~266 idle tokens total**, bringing full `efficient-ai` capabilities to IDE agents (Cursor, Windsurf, Cline, Copilot, Claude Desktop).
5. **Zero-Token Human Visibility (Antigravity):** `PostInvocation` injects `systemMessage: ""` + `metadata.user_markdown` savings badges so developers see live savings without spending a single LLM token.
6. **Prefix-Stable Batch Proxy (`eai proxy`):** Productizes Experiment 007 (`K=12, M=6` for Anthropic; `K=8, M=6` for OpenAI) behind `ANTHROPIC_BASE_URL` / `OPENAI_BASE_URL` with live mid-session cache-bust alerts.
