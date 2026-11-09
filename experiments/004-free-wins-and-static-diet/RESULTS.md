# Experiment 004: Free Wins — Static-Prefix Diet (H5), Prose Policy (H6), and CLI Flags (H8)

**Question:** How many tokens can we eliminate at **zero quality risk** before modifying any agent tools, simply by slimming the static prefix (**H5**), enforcing prose-only brevity (**H6**), and passing machine-friendly CLI flags (**H8**)?

**Method:** Run [`measure_free_wins.py`](measure_free_wins.py) on the dev machine's real instruction files and 70 installed skills, a 15-tool MCP server schema, paired assistant turn replies, and standard CLI command outputs.

**Run:** 2026-11-09, Python 3.12, `tiktoken 0.14.0` (`o200k_base`)

---

## 1. H5: Static-Prefix Diet (Instruction Files, Skills Metadata, MCP Schemas)

Every token in the static prefix is re-sent on every turn (54.4× average amplification in [Experiment 003](../003-session-token-economics/RESULTS.md)).

| Component | Before (tok/turn) | After (tok/turn) | Reduction | 50-Turn Raw Saved | 50-Turn Cached Cost Saved (`1×` eq.) |
|---|---:|---:|---:|---:|---:|
| Instruction file (`~/.claude/CLAUDE.md` → [`prompts/AGENTS.md`](../../prompts/AGENTS.md)) | 1,515 | 227 | **−85.0%** | 64,400 | 7,921 |
| Skills idle frontmatter (70 real skills: multi-line → 1-line description cap) | 5,811 | 2,061 | **−64.5%** | 187,500 | 23,063 |
| MCP server schema (15 tools: pretty + verbose → minified + terse) | 2,112 | 739 | **−65.0%** | 68,650 | 8,444 |
| **Total Static Prefix** | **9,438** | **3,027** | **−67.9%** | **320,550** | **39,428** |

---

## 2. H6: Prose-Only Brevity vs. Verbose vs. "Caveman"

| Style | `o200k_base` Tokens | Δ vs. Verbose | Echoes Code/Recaps? | Grammar & Precision Intact? |
|---|---:|---:|---|---|
| **Verbose (default assistant style)** | 297 | — | Yes (echoes diff + tool recap) | Yes |
| **H6 Prose-Only Brevity ([`prompts/AGENTS.md`](../../prompts/AGENTS.md))** | 103 | **−65.3%** | No (`path:line` citation only) | **Yes** |
| **Extreme 'Caveman' (broken grammar)** | 50 | **−83.2%** | No | No (drops articles/syntax) |

*Finding:* Most of the bloat in assistant replies comes from **echoing code blocks that were just edited** and **narrating tool calls that are already visible in the UI/trajectory**. Cutting those via [`prompts/policies/h6-prose-brevity.md`](../../prompts/policies/h6-prose-brevity.md) achieves **78.5% of Caveman's token reduction** (`−65.3%` vs. `−83.2%`) while keeping complete sentences and technical precision intact.

---

## 3. H8: Machine-Friendly CLI Flags

| Command Pair | Default (`o200k`) | Compact Flag (`o200k`) | Reduction | Info Lost |
|---|---:|---:|---:|---|
| `git status` vs. `git status -sb` | 149 | 49 | **−67.1%** | None (git usage hints only) |
| `git log -n 5` vs. `git log --oneline -n 5` | 325 | 89 | **−72.6%** | Author/date headers |
| `pytest -v` (20 passing) vs. `pytest -q --tb=short` | 463 | 15 | **−96.8%** | Per-test `PASSED` lines |
| `cargo test` (verbose) vs. `cargo test -q` | 245 | 41 | **−83.3%** | Per-test `... ok` lines |

---

## Key Takeaways

1. **Static-prefix hygiene saves ~320k raw context tokens per 50-turn session (−67.9%).** The biggest hidden offender is **multi-line YAML `description` fields in installed skills** (70 skills cost 5,811 tokens/turn idle; capping descriptions to one sentence cuts that to 2,061 tokens/turn).
2. **Prose-only brevity (−65.3%) is a safer default than "caveman" mode.** Instructing the model not to recap tool calls or echo edited code blocks captures nearly four-fifths of caveman's savings without risking malformed syntax in code or commands.
3. **Compact CLI flags (`-sb`, `--oneline`, `-q`) cut command output 67–97% with zero recovery loops** because failure tracebacks are still printed in full when tests fail. In Phase 3 (`squeeze`), we automate these flag rewrites so the agent doesn't even have to remember them.
