# efficient-ai

**Same quality, fewer tokens.**

`efficient-ai` is an open research project and toolkit for cutting the token and API cost of AI coding agents—with **Claude Code**, **Codex CLI**, and **Gemini CLI** supported as first-class targets from day one.

---

## Why This Exists

Popular "token killer" wrappers claim **60–90% token savings** by counting bytes stripped from a single command's `stdout`. When independent benchmarks test them across full agent sessions, those savings vanish—or reverse (**+7.6% higher cost** at low effort):

1. **Wrong metric:** A session's cost is driven by **re-sent conversation history across $T$ turns** (input tokens grow roughly quadratically in $T$) and **prompt-cache hit rates**, not one tool's raw byte count.
2. **Dilution:** Build, test, and `git` output make up only **~6%** of tool-output tokens in typical coding sessions.
3. **Recovery loops:** Lossy filtering drops the one line the agent needed. The agent re-runs the command, reads the whole file, or guesses wrong—burning far more tokens in extra turns than the filter saved.
4. **Cache busting:** Naive per-turn history pruning mutates the prefix and invalidates prompt caches (where cached reads cost **0.1×–0.5×** normal input).

**Our approach:** Measure **total session tokens and billed cost** on paired end-to-end benchmarks, require a **non-inferiority test on task solve rate**, and make every compression **lossless by default** (spill full output to disk with a one-line pointer so recovery is one cheap lookup, never a re-run).

---

## Key Findings So Far

### 1. File reading—not build/test output—drives ~64% of tool tokens 🧪

We analyzed **317 real Claude Code sessions (8.05M tool-output tokens)** in [`experiments/002-tool-output-attribution`](experiments/002-tool-output-attribution/RESULTS.md):

| Source | Calls | Share of Tool Tokens | Avg Tokens / Call |
|---|---:|---:|---:|
| **File reading (`Read` tool)** | 1,660 | **42.7%** | 2,069 |
| **File reading via shell (`cat`, `sed`)** | 1,731 | **~21.0%** | 978 |
| **Search (`grep`/`rg`/`find`/`ls` + `Grep`/`Glob`)** | 2,092 | **8.6%** | 334 |
| **MCP tools** | 1,603 | **8.3%** | 414 |
| **Build / test / VCS (`git`, `npm`, `node`, `python`)** | 2,386 | **~6.5%** | 219 |
| **All other shell / edit / web tools** | 5,863 | **~12.9%** | — |

- **Heavy tail on reads:** 62% of `Read` calls load the entire file. The **largest 10% of reads account for 40.9% of all read tokens** (p90 = 5,592 tokens; p99 = 12,276 tokens).
- **Implication:** Compressing `git` or test logs targets ~6% of tool tokens. The highest-leverage intervention is **structure-aware reading** (`outline`: symbol outlines and range reads before whole-file loads, plus search that returns the enclosing symbol).

---

### 2. Tabs don't save tokens (+1.9% worse); structured data formats save 38–61% 🧪

In [`experiments/001-tokenizer-baselines`](experiments/001-tokenizer-baselines/RESULTS.md), we tested common representation myths against `o200k_base` and `cl100k_base`:

| Category | Representation | `o200k_base` | Δ vs. Baseline | Verdict |
|---|---|---:|---:|---|
| **Code** | 4-space indentation (baseline) | 48,079 | — | Baseline |
| **Code** | 2-space indentation | 48,079 | **0.0%** | BPE merges space runs into single tokens |
| **Code** | Tab indentation | 48,983 | **+1.9%** | **Myth busted:** tabs cost *more* tokens |
| **Code** | Comments & docstrings stripped | 28,167 | **−41.4%** | Useful for read-only outlines, risky for edits |
| **Data** | Pretty JSON (`indent=2`) | 2,842 | — | Baseline |
| **Data** | Minified JSON | 1,762 | **−38.0%** | Free win for tool & MCP payloads |
| **Data** | Tabular (TOON / CSV) | 1,173 / 1,108 | **−58.7% / −61.0%** | Best for uniform record lists |

---

### 3. What actually works end-to-end (State of the Art)

From our [literature and benchmark review](docs/research/00-state-of-the-art.md) (🧪 = measured in this repo, ⚠️ = secondary benchmark report pending primary replication):

| Intervention | Layer | Claimed | End-to-End Session Impact | Quality Risk |
|---|---|---|---|---|
| **Observation masking** (hide old tool outputs) | History | — | **~−50% cost**, equal/better solve rate (SWE-bench Verified, 5 models) | Low |
| **Prompt-cache hygiene** (stable prefix, no dynamic timestamps) | Cost | −50–90% | **0.1× read price** (Anthropic), **0.5×** (OpenAI) | None |
| **Trajectory pruning (AgentDiet)** | History | — | **−40–60% input tokens, −21–36% cost**, no perf loss ⚠️ | Low–Med |
| **Structured data minification / tabular encoding** | Observations | 30–60% | **−38% (minified JSON) to −61% (CSV)** 🧪 | Low |
| **Terse output policy** ("be brief" / caveman) | Output | 65–75% | **~−8.5% total output**, solve rate unchanged ($p=0.82$) ⚠️ | Low |
| **Lossy CLI stdout filtering** (e.g., RTK) | Observations | 60–90% | **0% to +7.6% higher cost** (dilution + recovery loops) ⚠️ | Med |

---

## Measure Your Own Sessions in 30 Seconds

Want to see where your own Claude Code tokens go? Run the attribution script locally. It reads `~/.claude/projects/**/*.jsonl` on your machine and prints **aggregate counts only** (no paths, prompts, or file contents are output):

```bash
python -m venv .venv
# macOS / Linux:
.venv/bin/pip install tiktoken
.venv/bin/python experiments/002-tool-output-attribution/analyze.py

# Windows (PowerShell):
.venv\Scripts\pip install tiktoken
.venv\Scripts\python experiments/002-tool-output-attribution/analyze.py
```

To reproduce the tokenizer baseline benchmark:

```bash
.venv/bin/python experiments/001-tokenizer-baselines/measure.py
```

---

## Principles

1. **Measure end-to-end.** A technique only counts if it lowers *total session* tokens or billed cost on a real benchmark (Harbor / Terminal-Bench / SWE-bench), with paired runs and 95% bootstrap confidence intervals. "Bytes removed from one stream" doesn't count.
2. **Quality is non-negotiable.** Every change must pass a pre-declared non-inferiority test on task solve rate.
3. **Lossless by default.** When we shrink an observation, the full output stays on disk with a one-line pointer (e.g., `[312 passing tests elided; full log: .eai/last.log]`). Recovery costs one cheap `grep`, not a full re-run.
4. **Cache-aware.** Any history edit must respect prompt-cache prefix boundaries so raw token savings don't turn into API bill increases.
5. **Cheap rules only.** Every instruction added to `AGENTS.md` or `CLAUDE.md` is paid on every turn. It has to earn its place.

---

## Roadmap & Architecture

Every tool is designed to work portably across **Claude Code**, **Codex CLI**, and **Gemini CLI** (see [`docs/research/01-agent-surfaces.md`](docs/research/01-agent-surfaces.md) for the hook, instruction, and transcript capability matrix).

| Phase | Deliverable | Focus | Status |
|---|---|---|---|
| **0. Research & Instrument** | [`docs/research/`](docs/research/), [`experiments/001`](experiments/001-tokenizer-baselines/RESULTS.md), [`002`](experiments/002-tool-output-attribution/RESULTS.md), `tools/tokscope` | Full-session token & cache attribution, static-prefix audit (`AGENTS.md`, skills, MCP schemas), cross-agent schema fixtures | **In Progress** |
| **1. Free Wins** | `prompts/`, `tokscope audit` | Static-prefix diet (**H5**), prompt-cache hygiene, machine-friendly CLI flags (`--porcelain`, `-q`, **H8**), prose-only brevity rules (**H6**) | Planned |
| **2. Read Layer** | `tools/outline` (Rust + `tree-sitter`) | Symbol/signature outlines, range reads, and search-with-enclosing-symbol (**H4**) + turn-reduction policy (**H3**) targeting the 64% file-read share | Planned |
| **3. Observation Layer** | `tools/squeeze` (Rust + PATH shims/hooks) | Lossless spill-to-file command output compression (**H2**) and stuck-loop detection (**H7**) | Planned |
| **4. History Layer** | Harness experiment + config recipe | Cache-aware batch observation masking at fixed turn boundaries (**H1**) | Planned |
| **5. Public Leaderboard** | Benchmark report + reproducible configs | Paired end-to-end A/B scoreboard across techniques and agents | Planned |

---

## Documentation

- [`docs/research/00-state-of-the-art.md`](docs/research/00-state-of-the-art.md) — Literature review, mathematical cost model, why existing tools fail end-to-end, hypotheses **H1–H8**, and measurement methodology.
- [`docs/research/01-agent-surfaces.md`](docs/research/01-agent-surfaces.md) — Hooks, instruction files, PATH shim architecture, and usage accounting schemas for Claude Code, Codex CLI, and Gemini CLI.
- [`experiments/001-tokenizer-baselines/RESULTS.md`](experiments/001-tokenizer-baselines/RESULTS.md) — Code indentation, comment stripping, and structured data encoding benchmarks.
- [`experiments/002-tool-output-attribution/RESULTS.md`](experiments/002-tool-output-attribution/RESULTS.md) — Tool-output token attribution across 317 Claude Code sessions.

## Contributing & Privacy

Contributions of **anonymized aggregate tables** from running [`analyze.py`](experiments/002-tool-output-attribution/analyze.py) on your own sessions (especially test-heavy workloads, Codex CLI, or Gemini CLI) are very welcome. **Never commit raw session transcripts**—they contain private code, paths, and environment details.

## License

[Apache-2.0](LICENSE)
