# State of the Art: Making AI Agents Use Fewer Tokens Without Losing Quality

*Literature and tooling review, compiled 2026-11-01. This is a living document.*

> **Reliability note.** Numbers marked ⚠️ come from secondary sources (blogs, aggregators) and still need to be checked against the primary source. Numbers marked 🧪 come from our own experiments in `experiments/`.

---

## 0. TL;DR

1. **Tokens in an agent session mostly come from *re-sent history*, not from new text.** Each turn re-sends the whole conversation. Tool observations make up about 84% of trajectory length (JetBrains, *The Complexity Trap*). So the biggest savings come from **fewer turns** and **less material that stays in history**, not from shorter replies.
2. **Popular "token killer" tools mostly fail end-to-end.** Independent paired benchmarks found:
   - RTK (60–90% claimed) gave **no end-to-end cost reduction**, and was **+7.6% more expensive** at low effort ⚠️ (Quesma, JetBrains).
   - The "caveman" output skill (65% claimed) gave **~8.5%** in real coding tasks, with no quality loss ⚠️ (JetBrains). A plain "be brief" prompt did about as well ⚠️.
   - **Why:** they measure bytes saved in one stream, ignore dilution, and cause *recovery loops* (lost info leads to extra turns, and those cost more than was saved).
3. **Interventions with solid end-to-end evidence:**
   - **Observation masking** (hide old tool outputs): ~50% cost reduction, solve rate unchanged (JetBrains, NeurIPS'25 DL4Code).
   - **Trajectory pruning** (AgentDiet): −40–60% input tokens, −21–36% cost, no performance loss ⚠️.
   - **Code-execution-over-MCP / tool search**: up to ~98% on tool-heavy workflows (Anthropic).
   - **Prompt-cache hygiene**: cache reads are ~0.1× input price on Anthropic.
4. **The real gap in the field is *rigorous measurement*, not one more compressor.** This project's edge: every technique ships with a **paired, end-to-end A/B on a public benchmark**, with token accounting taken from provider usage fields and a quality non-inferiority test.
5. 🧪 Our first experiment debunks a common myth. **Tabs and 2-space indentation don't save tokens** (tabs cost +1.9% on o200k). The cheap wins are in structured-data encoding (minified JSON −38%, tabular −60%) and comments/docstrings (−41%, but those carry meaning).

---

## 1. Cost model: where tokens actually go

For a session of $T$ turns, where turn $t$ sends a context of $C_t$ tokens and generates $O_t$ tokens:

$$
\text{Cost} \approx \sum_{t=1}^{T} \Big( p_{\text{read}}\,C_t^{\text{cached}} + p_{\text{write}}\,C_t^{\text{new}} + p_{\text{in}}\,C_t^{\text{uncached}} + p_{\text{out}}\,(O_t + R_t) \Big)
$$

Here $R_t$ is reasoning tokens, and $C_t \approx S + \sum_{i<t} (\text{obs}_i + O_i)$, where $S$ is the static prefix (system prompt, tool schemas, AGENTS.md, skills metadata).

What follows from this:

| Lever | Why it matters |
|---|---|
| **Number of turns $T$** | Total input grows roughly **quadratically** in $T$. Removing one turn early saves its context in every later turn. |
| **Persistent observations** | A 5k-token file read on turn 3 of a 40-turn session is re-sent 37 times. |
| **Static prefix $S$** | Paid on every turn. Cached, but still counts against context and quality ("context rot"). |
| **Cache hit rate** | Anthropic: cache read ≈ 0.1× input, write 1.25× (5 min) or 2× (1 h). OpenAI: ~50% discount, automatic. Gemini: implicit ~75% discount ⚠️. Any byte change in the prefix invalidates everything after it. |
| **Output + reasoning** | Priced ~4–5× input, but a small share of volume in coding agents. |

**Tension to study.** Pruning or masking history *changes the prefix* and so **breaks the prompt cache**. A pruning strategy can lower raw tokens but raise cost if it busts the cache every turn. Pruning in batches (every N turns, at fixed boundaries) is likely the right compromise. **This is an open research question, and a key experiment for us.**

**Two metrics, not one.** We should report **raw tokens** (what fills context and drives subscription usage limits and quality) and **cost-weighted tokens** (what drives API bills) separately. Many claims online mix the two up.

---

## 2. Evidence table

| # | Technique | Layer | Claimed | Independent end-to-end evidence | Quality risk |
|---|---|---|---|---|---|
| 1 | **Observation masking** (replace old tool outputs with placeholders) | history | — | **~−50% cost, equal/better solve rate**, SWE-agent on SWE-bench Verified, 5 models. LLM summarization was no better: it cost +~7% for summary calls and made trajectories **~15% longer** because it hides failure signals. A hybrid of the two gives another 7–11%. [JetBrains, *The Complexity Trap*, arXiv:2508.21433] | Low |
| 2 | **AgentDiet** trajectory reduction (drop useless / redundant / expired info) | history | — | −39.9–59.7% input tokens, −21.1–35.9% cost, no perf loss ⚠️ | Low–Med |
| 3 | **Code execution with MCP / programmatic tool calling** | tools | up to −98% | Anthropic engineering posts. Intermediate data stays in a sandbox and never enters context. | Med (needs sandbox) |
| 4 | **Tool search / lazy tool loading** | static prefix | large | MCP schemas can cost tens of thousands of tokens up front. Load on demand instead. | Low |
| 5 | **Progressive disclosure (Agent Skills)** | static prefix | ~100 tok/skill idle | Standard across Claude Code, Gemini CLI, Antigravity. | Low |
| 6 | **Prompt-cache hygiene** (static content first, no timestamps in prefix) | cost | −50–90% on input $ | Provider docs. | None |
| 7 | **Symbol-level code access** (LSP/tree-sitter: Serena, aider repo-map) | reads | 50–90% on exploration ⚠️ | Mostly anecdotal. Aider's repo map fits a PageRank-ranked map into ~1k tokens. | Med (missed context, extra turns) |
| 8 | **Diff-based edits** (search/replace, udiff) vs whole-file rewrites | output | large on big files | Aider benchmarks. udiff cut GPT-4 Turbo "laziness" ~3×. | Low for frontier models |
| 9 | **Tool-output filtering** (RTK) | observations | 60–90% of the stream | **No end-to-end saving. +7.6% cost at low effort, neutral at high** ⚠️ (Quesma, JetBrains). `rtk gain` estimates bytes/4 of one stream. | Med (recovery loops) |
| 10 | **Terse prose** ("caveman", "be brief") | output | 65–75% | **~8.5% total output** on SkillsBench, quality p=0.82 ⚠️. "Be brief" ≈ caveman ⚠️. | Low |
| 11 | **Chain of Draft** (≤5 words per reasoning step) | reasoning | — | 7.6–20% of CoT tokens at equal accuracy on GSM8K, commonsense, symbolic tasks [arXiv:2502.18600]. **Not validated on agentic coding.** | Med on hard tasks |
| 12 | **TALE** token-budget-aware reasoning | reasoning | — | Large reduction, small accuracy loss [arXiv:2412.18547]. Rigid budgets hurt. | Med |
| 13 | **Effort / verbosity knobs** (Claude `effort`, OpenAI `reasoning.effort`, `verbosity`) | reasoning/output | — | Native and cheap. Returns diminish above "high". Lower verbosity before lowering effort. | Med if effort is cut |
| 14 | **LLMLingua-2** prompt compression | inputs | 2–5× | Strong on natural language (MeetingBank, LongBench). **Breaks code syntax** [arXiv:2403.12968] | High for code |
| 15 | **LongCodeZip** (function, then block pruning by conditional perplexity) | inputs | up to 5.6× | Code completion, summarization, RepoQA (ASE'25). Needs a local LM. | Med |
| 16 | **Structured-data encodings** (minified JSON, TSV/TOON) | observations | 30–60% | 🧪 −38% (minified) / −59% (TOON-like) / −61% (CSV) vs pretty JSON. TOON comprehension ≈ JSON on retrieval ⚠️. | Low for input, Med for output |
| 17 | **Sub-agents** for context isolation | architecture | — | Keeps main context clean, **but multi-agent ≈ 15× chat tokens**, single agent ≈ 4× (Anthropic research system). Net-positive only if the main thread would otherwise carry the junk. | Cost ↑ |
| 18 | **Context rot** (justification) | — | — | Chroma: all 18 models tested degrade as input grows, more so with distractors. **Fewer tokens can raise quality.** | — |

---

## 3. Why existing tools fail end-to-end (diagnosis)

1. **Measuring the wrong thing.** They count bytes removed from one stream, not billed tokens across the session.
2. **Dilution.** Bash output is a minor share of input. Cutting it 90% moves the total by a few percent.
3. **Recovery loops.** Lossy compression drops the one line the agent needed. The agent re-runs the command, `cat`s the file, or guesses wrong, and that costs more than was saved.
4. **Instruction tax.** Every rule added to the system prompt or AGENTS.md is paid on every turn. A compression tool needs a usage manual for the model, and that manual eats into its own gains.
5. **Cache interactions.** Changing earlier content busts the prefix cache.
6. **Tiny samples, no pairing.** Agent runs are high-variance. Single-run comparisons are noise.

**Design principle for this repo:** *lossless-by-default, retrievable compression*. When we shrink something, we keep the full version on disk and leave a one-line pointer, for example `[312 passing tests elided; full log: .eai/last.log]`. Recovery then costs one cheap `grep` instead of a full re-run.

---

## 4. Opportunity map: hypotheses for this project

Ranked by expected impact × confidence. Each one becomes an experiment with a pre-registered metric.

| ID | Hypothesis | Mechanism | Deliverable |
|---|---|---|---|
| **H1** | Cache-aware observation masking beats both no-masking and per-turn masking on *cost*, with equal solve rate. | Mask old tool outputs in batches at fixed boundaries to keep cache prefixes stable. Compare with provider-native context editing where available. | 🧪 **Confirmed in [Exp 007](../../experiments/007-cache-aware-observation-masking/RESULTS.md):** Per-turn masking raises Claude cost **+9.2% to +111.8%**; batch masking ($K=10\text{–}15, M=6$) cuts raw tokens **−22.9% to −38.2%** and lowers cost **−9.6% to −10.5% (Claude)** / **−21.0% to −34.8% (Codex)** |
| **H2** | Retrievable compression (spill to file + pointer) gets RTK-level stream reduction **without** recovery loops. | Rust binary wraps commands: strips ANSI/progress, dedupes repeated lines with counts, extracts failures, tees the full output to a file. | 🧪 **Confirmed in [Exp 006](../../experiments/006-squeeze-retrievable-compression/RESULTS.md):** [`tools/squeeze`](../../tools/squeeze/) achieves **−92.6%** stream reduction while preserving 100% of error/warning signals and spilling full logs to `.eai/logs/last.log` |
| **H3** | Turn-count reduction is the strongest single lever. | Prompt policy: batch and parallelize independent reads, read ranges and outlines before whole files, stop after verification. Measure turns/task. | [`prompts/AGENTS.md`](../../prompts/AGENTS.md) + `outline search --expand` |
| **H4** | Outline-first reads cut read tokens >50% at equal quality. | Structural symbol outline (signatures + line numbers) and range/symbol reads. | 🧪 **Confirmed in [Exp 005](../../experiments/005-outline-read-layer/RESULTS.md):** [`tools/outline`](../../tools/outline/) cuts file-explore tokens **−89.7%**, explore+edit **−72.7%**, and corpus-wide tool tokens **−25.2%** |
| **H5** | Static-prefix diet: auditing system prompt, AGENTS.md, MCP schemas, and skills saves tokens on every turn at zero quality cost. | Token audit tool + rewrite guidelines. | 🧪 **Confirmed in [Exp 004](../../experiments/004-free-wins-and-static-diet/RESULTS.md):** `tokscope audit` + static diet cuts idle prefix **−67.9%** (`9,438 → 3,027` tok/turn, saving `320k` raw tokens per 50-turn session) |
| **H6** | Output policy limited to *prose* (no recaps, no echoing code, diffs only) gets most of caveman's gain with less risk. | Short rules, tested against "be brief" and caveman. | 🧪 **Confirmed in [Exp 004](../../experiments/004-free-wins-and-static-diet/RESULTS.md):** Cuts assistant prose **−65.3%** (79% of caveman's savings) with full grammar intact |
| **H7** | Loop / "token burn" detection saves a lot on failing tasks. | Wrapper detects repeated identical command+error N times and injects a nudge to change strategy. | Built into [`tools/squeeze`](../../tools/squeeze/) (`.eai/loop_state.txt`) |
| **H8** | Machine-friendly tool flags (`--porcelain`, `--json` minified, `-q`) are a free win. | Command-rewrite table in `squeeze --rewrite` and pre-tool hooks. | 🧪 **Confirmed in [Exp 004](../../experiments/004-free-wins-and-static-diet/RESULTS.md):** **−67% to −97%** on `git`/`pytest`/`cargo` + automated in [`hooks/`](../../hooks/) |

**Target agents: Claude Code, Codex CLI, and Gemini CLI, all from the start.** The full capability matrix is in [`01-agent-surfaces.md`](01-agent-surfaces.md). Summary:
- Codex **can't rewrite** tool args (`updatedInput` is rejected) ⚠️.
- Claude Code can't replace **built-in** tool output (MCP only).
- Gemini CLI can do both ⚠️.

So the only portable interception point is **wrapping the command itself**, primarily via **PATH shims**, with per-agent rewrite hooks as a secondary layer.

---

## 5. Measurement methodology (the core of the project)

**Metrics per task run:**
- input uncached, cache-write, cache-read, output, and reasoning tokens (from provider `usage` fields, not estimates)
- number of turns and tool calls
- wall time
- list-price cost
- **task success** (verifier pass/fail)

**Harness:**
- **Harbor** + Terminal-Bench 2.x and SkillsBench for real CLI agents (Claude Code, Codex, Gemini CLI). These are the same setups JetBrains and Quesma used, so our results are directly comparable.
- `mini-swe-agent` on a fixed SWE-bench Verified subset for cheap, fully controlled experiments where we own the agent loop (needed for H1).
- Transcript parsing for real-world sessions: Claude Code JSONL under `~/.claude/projects/`, Codex `~/.codex/sessions/`. **Never commit raw transcripts.**

**Statistics:**
- Paired design: same tasks, same model/version, ≥3 trials per arm.
- Report paired bootstrap 95% CIs for Δcost and Δtokens.
- Quality is a **non-inferiority test** with a pre-declared margin (e.g. solve rate no worse than −2 pp).
- Report "% of total session tokens", never "% of one stream".

**Tokenizers:** tiktoken (o200k/cl100k) for fast offline proxies. Anthropic and Gemini count-tokens endpoints for final numbers.

---

## 6. Proposed repository layout and roadmap

```
efficient-ai/
├── docs/research/        # this review, experiment write-ups, decisions
├── experiments/NNN-*/    # one folder per experiment: script + RESULTS.md (aggregates only)
├── tools/
│   ├── tokscope/         # transcript parser + token attribution + static-prefix audit (Python first)
│   ├── squeeze/          # Rust: retrievable tool-output compression + command rewriting
│   └── outline/          # Rust: tree-sitter outlines and range reads
├── hooks/                # Claude Code / Codex / Gemini CLI hook adapters
├── prompts/              # AGENTS.md / CLAUDE.md policy snippets, each A/B-tested
└── data/public/          # sanitized, aggregated datasets only
```

> 🧪 Ordering is informed by [experiment 002](../../experiments/002-tool-output-attribution/RESULTS.md) and [experiment 003](../../experiments/003-session-token-economics/RESULTS.md): across 13,370 API turns (`2.59B` total tokens), each new input token is re-sent **54.4×** (`98.16%` cache hit rate). File reading is ~64% of first-occurrence tool tokens and ~57% of cumulative re-sent exposure, while build/test/git output is ~6%. So the read layer comes before the observation layer.

| Phase | Goal | Exit criterion |
|---|---|---|
| **0. Instrument** ✅ | `tokscope`: attribute every token in a session to static prefix / user / tool output (by tool) / assistant / reasoning; cache hit rate | Breakdown of ≥20 real sessions; know our actual distribution (**Done:** 315 transcripts / 13,370 turns in [Exp 003](../../experiments/003-session-token-economics/RESULTS.md)) |
| **1. Free wins** ✅ | H5 (static diet), cache hygiene, H8, H6 | [`prompts/`](../../prompts/) + [Exp 004](../../experiments/004-free-wins-and-static-diet/RESULTS.md) (**−67.9%** idle static prefix, **−65.3%** prose, **−67–97%** compact CLI flags) |
| **2. Read layer** ✅ | H4 `outline` (skim/symbol/range reads, plus search that returns the enclosing symbol), H3 turn reduction | [`tools/outline`](../../tools/outline/) + [Exp 005](../../experiments/005-outline-read-layer/RESULTS.md) (**−89.7%** explore, **−72.7%** explore+symbol, **−25.2%** corpus tool tokens) |
| **3. Observation layer** ✅ | H2 `squeeze` + hook; H7 loop detection | [`tools/squeeze`](../../tools/squeeze/) + [`hooks/`](../../hooks/) + [Exp 006](../../experiments/006-squeeze-retrievable-compression/RESULTS.md) (**−92.6%** stream tokens, 100% error preservation, `.eai/logs/last.log` spill) |
| **4. History layer** ✅ | H1 cache-aware masking | [Exp 007](../../experiments/007-cache-aware-observation-masking/RESULTS.md) (batch masking $K=10\text{–}15, M=6$ cuts raw tokens **−22.9% to −38.2%** and lowers Claude cost **−9.6% to −10.5%** vs. **+9.2% to +111.8%** for per-turn masking) |
| **5. Publish** ✅ | Technique leaderboard with honest end-to-end numbers | [`README.md`](../../README.md) leaderboard + [`data/public/`](../../data/public/README.md) sanitized 315-session dataset |

**Toolchain:** Rust 1.99.1 (stable, MSVC), Python 3.12, Node, git, jq. Codex CLI and Gemini CLI still need to be installed for schema verification.

---

## 7. Public-repo hygiene

- **Never publish:** API keys and `.env` files; raw agent transcripts (they contain private code, absolute paths, usernames, sometimes secrets echoed by tools); raw Harbor/SWE-bench run directories; personal agent configs (`settings.local.json`, `auth.json`).
- **Publish:** code, docs, prompt snippets, hook configs, experiment scripts, and **aggregated** results (CSV/MD of per-task token counts with task IDs only).
- **Before the first push:** add `gitleaks` (or `trufflehog`) as a pre-commit hook and in CI; add a `sanitize` step to `tokscope` that strips paths and contents before anything enters `data/public/`.
- **Third-party content:** don't vendor RTK/caveman/LLMLingua code. Reference and benchmark them. Benchmark task data stays under its own license; we only publish IDs and metrics.
- License: **Apache-2.0** (`LICENSE` + `NOTICE`). Add `CONTRIBUTING.md` (with DCO sign-off) once there's code.

---

## 8. Key references

- JetBrains Research. *The Complexity Trap: Simple Observation Masking Is as Efficient as LLM Summarization for Agent Context Management.* NeurIPS'25 DL4Code. arXiv:2508.21433
- *AgentDiet:* trajectory reduction for LLM agents (useless / redundant / expired information). arXiv, 2025 ⚠️ verify ID
- Anthropic Engineering. *Effective context engineering for AI agents*; *Code execution with MCP*; *Advanced tool use (Tool Search, Programmatic Tool Calling)*; *How we built our multi-agent research system.*
- Chroma. *Context Rot: How Increasing Input Tokens Impacts LLM Performance* (Hong, Troynikov, Huber).
- Xu et al. *Chain of Draft: Thinking Faster by Writing Less.* arXiv:2502.18600
- Han et al. *Token-Budget-Aware LLM Reasoning (TALE).* arXiv:2412.18547
- Pan et al. *LLMLingua-2.* arXiv:2403.12968
- *LongCodeZip: Compress Long Context for Code Language Models.* ASE 2025
- Aider docs: *Repository map*; *Edit formats*; *Unified diffs make GPT-4 Turbo 3× less lazy.*
- JetBrains Blog (2026). Evaluation of the caveman skill and token-saver tools on SkillsBench via Harbor ⚠️
- Quesma Blog. *RTK reports huge token savings, but our cost benchmarks disagree* ⚠️
- rtk-ai/rtk (GitHub); JuliusBrussee/caveman (GitHub); oraios/serena (GitHub); TOON spec (toon-format)
- Harbor / Terminal-Bench (tbench.ai)
