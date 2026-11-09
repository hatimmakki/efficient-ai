# efficient-ai

**Same quality, fewer tokens.**

`efficient-ai` is an open research project and toolkit for cutting the token and API cost of AI coding agents—with **Claude Code**, **Codex CLI**, and **Gemini CLI** supported as first-class targets from day one.

---

## Why This Exists

Popular "token killer" wrappers claim **60–90% token savings** by counting bytes stripped from a single command's `stdout`. When independent benchmarks test them across full agent sessions, those savings vanish—or reverse (**+7.6% higher cost** at low effort):

1. **Wrong metric:** A session's cost is driven by **re-sent conversation history across $T$ turns** (input tokens grow roughly quadratically in $T$) and **prompt-cache hit rates**, not one tool's raw byte count.
2. **Dilution:** Build, test, and `git` output make up only **~6–7%** of tool-output tokens in typical coding sessions.
3. **Recovery loops:** Lossy filtering drops the one line the agent needed. The agent re-runs the command, reads the whole file, or guesses wrong—burning far more tokens in extra turns than the filter saved.
4. **Cache busting:** Naive per-turn history pruning mutates the prefix and invalidates prompt caches (where cached reads cost **0.1×–0.5×** normal input).

**Our approach:** Measure **total session tokens and billed cost** on paired end-to-end benchmarks, require a **non-inferiority test on task solve rate**, and make every compression **lossless by default** (spill full output to disk with a one-line pointer so recovery is one cheap lookup, never a re-run).

---

## Key Findings So Far

### 1. Each new token is re-sent 54.4×; cache reads are 66.7% of billed cost 🧪

Using [`tokscope`](tools/tokscope/) across **315 real Claude Code transcripts (66 parent sessions + 249 sub-agent runs, 13,370 deduplicated API turns, 2.59B total tokens)** in [`experiments/003-session-token-economics`](experiments/003-session-token-economics/RESULTS.md):

| Component | Raw Tokens | Share of Raw | Cost-Weighted Tokens (`1×` = base input) | Share of Billed Cost |
|---|---:|---:|---:|---:|
| **Cache read (`0.1×`)** | 2,532,482,885 | 97.78% | 253,248,288 | **66.7%** |
| **Cache write (`1.25×` 5m / `2.0×` 1h)** | 47,374,972 | 1.83% | 76,264,021 | **20.1%** |
| **Output + thinking (`5.0×`)** | 10,012,669 | 0.39% | 50,063,345 | **13.2%** |
| **Uncached input (`1.0×`)** | 30,258 | 0.001% | 30,258 | **0.01%** |
| **Total** | **2,589,900,784** | **100.0%** | **379,605,912** | **100.0%** |

- **54.4× History Amplification:** Only `47.4M` new input tokens entered the context, but re-sending history across turns produced `2.58B` total input tokens (`98.16%` prompt-cache hit rate).
- **Why per-turn history pruning backfires:** Because cached reads cost `0.1×` and cache writes cost `1.25×–2.0×`, mutating the prefix every turn increases token cost **12.5×–20×** on the invalidated prefix. History masking must happen in **batches at fixed turn boundaries**.
- **Sub-agents are 31.5% of total cost:** Sub-agent trajectories (`isSidechain: true`) accounted for **45.3% of turns**, **49.2% of cache writes**, and **31.5% of total cost**—and the summaries they return to parent orchestrator sessions are re-sent **233×** on average.

---

### 2. File reading—not build/test output—drives ~64% of tool tokens (and 57% of re-sent exposure) 🧪

In [`experiments/002-tool-output-attribution`](experiments/002-tool-output-attribution/RESULTS.md) and [`experiments/003-session-token-economics`](experiments/003-session-token-economics/RESULTS.md), we tracked both first-occurrence tool tokens ($1\times$) and cumulative re-sent exposure across remaining turns ($\sum (T-t)\cdot\text{tok}$):

| Source | Calls | First-Occurrence Share ($1\times$) | Re-sent History Share ($\sum (T-t)\cdot\text{tok}$) | Avg Amplification |
|---|---:|---:|---:|---:|
| **File reading (`Read` tool)** | 1,527 | **41.4%** | **32.0%** | 47.9× |
| **File reading via shell (`cat`, `sed`, `head`)** | 1,747 | **22.5%** | **25.0%** | 69.1× |
| **Search (`grep`/`rg`/`find`/`ls` + `Grep`/`Glob`)** | 2,011 | **9.0%** | **9.3%** | 64.3× |
| **MCP tools** | 1,330 | **7.9%** | **7.6%** | 60.0× |
| **Sub-agent returned results** | 261 | **0.9%** | **3.4%** | **233.0×** |
| **Build / test / VCS (`git`, `npm`, `node`, `python`)** | 2,336 | **~6.8%** | **~7.0%** | 64.2× |

- **Heavy tail on reads:** 62.8% of `Read` calls load the entire file. The **largest 10% of reads account for 41.2% of all read tokens** (p90 = 5,357 tokens; p99 = 12,409 tokens).
- **Implication:** Compressing `git` or test logs targets ~7% of tool output. The highest-leverage tool to build first is **structure-aware reading** (`outline`: symbol outlines and range reads before whole-file loads, plus search that returns the enclosing symbol).

---

### 3. Tabs don't save tokens (+1.9% worse); structured data formats save 38–61% 🧪

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

### 4. What actually works end-to-end (State of the Art)

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

## Measure Your Own Sessions in 30 Seconds (`tokscope`)

Want to see where your own agent tokens and API dollars go, or audit your `AGENTS.md` / `CLAUDE.md` / skills prefix overhead? Run [`tokscope`](tools/tokscope/) locally (100% offline; prints **aggregate counts only**, never file contents or paths):

```bash
python -m venv .venv
# macOS / Linux:
.venv/bin/pip install -e tools/tokscope
.venv/bin/tokscope analyze                   # Full-session token economics & history amplification
.venv/bin/tokscope audit . --include-user    # Static-prefix token audit (AGENTS.md, skills, MCP)

# Windows (PowerShell):
.venv\Scripts\pip install -e tools/tokscope
.venv\Scripts\tokscope analyze
.venv\Scripts\tokscope audit . --include-user
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
| **0. Research & Instrument** | [`docs/research/`](docs/research/), [`experiments/001`](experiments/001-tokenizer-baselines/RESULTS.md)–[`003`](experiments/003-session-token-economics/RESULTS.md), [`tools/tokscope`](tools/tokscope/) | Full-session token & cache attribution, static-prefix audit (`AGENTS.md`, skills, MCP schemas), cross-agent schema fixtures | **Done** ✅ |
| **1. Free Wins** | `prompts/`, `tokscope audit` | Static-prefix diet (**H5**), prompt-cache hygiene, machine-friendly CLI flags (`--porcelain`, `-q`, **H8**), prose-only brevity rules (**H6**) | **Next** |
| **2. Read Layer** | `tools/outline` (Rust + `tree-sitter`) | Symbol/signature outlines, range reads, and search-with-enclosing-symbol (**H4**) + turn-reduction policy (**H3**) targeting the 64% file-read share | Planned |
| **3. Observation Layer** | `tools/squeeze` (Rust + PATH shims/hooks) | Lossless spill-to-file command output compression (**H2**) and stuck-loop detection (**H7**) | Planned |
| **4. History Layer** | Harness experiment + config recipe | Cache-aware batch observation masking at fixed turn boundaries (**H1**) | Planned |
| **5. Public Leaderboard** | Benchmark report + reproducible configs | Paired end-to-end A/B scoreboard across techniques and agents | Planned |

---

## Documentation

- [`docs/research/00-state-of-the-art.md`](docs/research/00-state-of-the-art.md) — Literature review, mathematical cost model, why existing tools fail end-to-end, hypotheses **H1–H8**, and measurement methodology.
- [`docs/research/01-agent-surfaces.md`](docs/research/01-agent-surfaces.md) — Hooks, instruction files, PATH shim architecture, and usage accounting schemas for Claude Code, Codex CLI, and Gemini CLI.
- [`tools/tokscope/`](tools/tokscope/README.md) — CLI for transcript attribution, history amplification modeling, static-prefix auditing, and privacy sanitization.
- [`experiments/001-tokenizer-baselines/RESULTS.md`](experiments/001-tokenizer-baselines/RESULTS.md) — Code indentation, comment stripping, and structured data encoding benchmarks.
- [`experiments/002-tool-output-attribution/RESULTS.md`](experiments/002-tool-output-attribution/RESULTS.md) — First-occurrence tool-output token attribution.
- [`experiments/003-session-token-economics/RESULTS.md`](experiments/003-session-token-economics/RESULTS.md) — Full-session cost-weighted token economics, 54.4× history amplification, and sub-agent cost breakdown across 13,370 API turns.

## Contributing & Privacy

Contributions of **anonymized aggregate tables** from running [`tokscope analyze`](tools/tokscope/README.md) on your own sessions (especially test-heavy workloads, Codex CLI, or Gemini CLI) are very welcome. **Never commit raw session transcripts**—they contain private code, paths, and environment details.

## License

[Apache-2.0](LICENSE)
