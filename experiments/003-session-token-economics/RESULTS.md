# Experiment 003: Full-Session Token Economics & History Amplification

**Question:** Once we account for **re-sent history across turns**, **prompt caching (`0.1×` reads vs. `1.25×/2.0×` writes)**, **output pricing (`5.0×`)**, and **sub-agents**, where do raw tokens and actual API dollars go across real sessions?

**Method:** Run [`tokscope analyze`](../../tools/tokscope/) and [`tokscope audit --include-user`](../../tools/tokscope/) on 315 real Claude Code transcripts (66 parent sessions + 249 sub-agent trajectories, 13,370 deduplicated API turns).

**Run:** 2026-11-09, `tokscope 0.1.0`, Python 3.12, `tiktoken 0.14.0`

---

## 1. Corpus Overview (13,370 API Turns)

- **Transcripts analyzed:** 315 non-empty transcripts (**66 parent sessions**, **249 sub-agent runs**; 2 empty session files skipped).
- **Deduplicated API turns:** **13,370** (7,312 parent turns, 6,058 sub-agent turns).
- **Streaming lines merged:** **13,788** duplicate per-block JSONL lines merged by `message.id` / `requestId`.
- **Prompt-cache hit rate:** **98.16%** (`2.53B` cache-read tokens out of `2.58B` total input tokens).
- **History amplification factor:** **54.4×** — only `47.4M` new input tokens were written across all sessions, but they were re-sent across turns to produce `2.58B` total input context tokens.
- **Sub-agent overhead (H17):** Sub-agent trajectories (`isSidechain: true`) account for **45.3% of all turns** and **31.5% of total cost-weighted tokens**.

---

## 2. Raw Tokens vs. Cost-Weighted Tokens

Reporting raw tokens alone hides where the API bill comes from. Using standard Anthropic cost weights normalized to `1.0× = 1 uncached input token` (`cache_read = 0.1×`, `cache_write_5m = 1.25×`, `cache_write_1h = 2.0×`, `output = 5.0×`):

| Component | Raw Tokens | Share of Raw | Cost-Weighted Tokens | Share of Cost |
|---|---:|---:|---:|---:|
| **Cache read (`0.1×`)** | 2,532,482,885 | 97.78% | 253,248,288 | **66.7%** |
| **Cache write (`1.25×` 5m / `2.0×` 1h)** | 47,374,972 | 1.83% | 76,264,021 | **20.1%** |
| **Output + thinking (`5.0×`)** | 10,012,669 | 0.39% | 50,063,345 | **13.2%** |
| **Uncached input (`1.0×`)** | 30,258 | 0.001% | 30,258 | **0.01%** |
| **Total** | **2,589,900,784** | **100.0%** | **379,605,912** | **100.0%** |

*Note on cache writes:* Of the `47.37M` cache-write tokens, `24.68M` (52.1%) used the 5-minute TTL (`1.25×`) and `22.71M` (47.9%) used the 1-hour TTL (`2.0×`).

---

## 3. First-Occurrence vs. Re-sent History Exposure by Tool

[Experiment 002](../002-tool-output-attribution/RESULTS.md) counted each tool output once ($1\times$). Here `tokscope` weights every tool output at turn $t$ by the number of remaining turns $(T - t)$ in that trajectory that re-send it:

| Category | Calls | First-Occurrence Tokens | Share (1×) | Re-sent Exposure ($\sum (T-t)\cdot\text{tok}$) | Share (Re-sent) | Avg Amplification |
|---|---:|---:|---:|---:|---:|---:|
| **Read (tool)** | 1,527 | 3,133,878 | 41.4% | 150,256,036 | **32.0%** | 47.9× |
| **Read (shell: `cat`/`sed`/`head`)** | 1,747 | 1,700,248 | 22.5% | 117,421,241 | **25.0%** | 69.1× |
| **Shell (other)** | 4,299 | 1,105,917 | 14.6% | 89,954,633 | **19.2%** | 81.3× |
| **Search (shell: `grep`/`rg`/`find`/`ls`)** | 1,676 | 550,639 | 7.3% | 37,789,989 | **8.1%** | 68.6× |
| **MCP** | 1,330 | 595,973 | 7.9% | 35,756,359 | **7.6%** | 60.0× |
| **Sub-agent result** | 261 | 68,147 | 0.9% | 15,878,718 | **3.4%** | **233.0×** |
| **Edit/Write** | 2,164 | 98,961 | 1.3% | 9,842,600 | **2.1%** | 99.5× |
| **Search (`Grep`/`Glob` tools)** | 335 | 126,137 | 1.7% | 5,745,828 | **1.2%** | 45.6× |
| **WebSearch + WebFetch** | 440 | 164,822 | 2.2% | 3,454,956 | **0.7%** | 21.0× |

---

## 4. Static-Prefix Audit (`tokscope audit --include-user`)

| Category | Source | Loaded | `o200k_base` | `cl100k_base` | Notes |
|---|---|---|---:|---:|---|
| User global instruction | `~/.claude/CLAUDE.md` | always (every turn) | 1,515 | 1,508 | Global prefix |
| User global skills (idle) | `~/.claude/skills` (25 skills) | always (every turn) | 2,736 | 2,755 | Avg 109 tok/skill idle vs. 90,551 tok full bodies (**−97.0%**) |
| User global skills (idle) | `~/.gemini/config/skills` (45 skills) | always (every turn) | 3,075 | 3,086 | Avg 68 tok/skill idle vs. 72,325 tok full bodies (**−95.7%**) |

---

## 5. Key Findings

1. **Prompt caching (`98.16%` hit rate) changes every optimization trade-off.** Even with a 90% discount (`0.1×`), **cache reads are 66.7% of total session cost** because every new token is re-sent **54.4×** on average. Meanwhile, any history pruning or compression that mutates the prefix *every turn* converts `0.1×` cache reads into `1.25×–2.0×` cache writes—increasing cost by **12.5×–20×** on the invalidated prefix. This confirms **H1**: observation masking must run in **batches at fixed turn boundaries**, never every turn.
2. **File reading remains #1 after modeling history re-sending (~57.0% of re-sent exposure, ~63.9% of first-occurrence tokens).** Moreover, shell file reads (`cat`, `sed`) have higher amplification (**69.1×**) than `Read` tool calls (**47.9×**) because agents run exploratory shell reads early in long sessions.
3. **Sub-agents are a major hidden cost center (31.5% of total cost, 45.3% of turns) AND their returned summaries amplify 233× in parent sessions.** Spawning sub-agents keeps intermediate reads out of the parent context, but each sub-agent pays its own cache-write prefix cost (`23.3M` of the `47.4M` cache writes came from sub-agents), and the summary returned to the parent is re-sent ~233 times in long orchestrator sessions.
4. **Two critical Claude Code transcript-accounting bugs uncovered and fixed in `tokscope`:**
   - **Streaming `output_tokens` placeholders:** When Claude Code logs a multi-block assistant turn (e.g., `thinking` → `text` → `tool_use`), each content block is written as a separate JSONL line with the same `message.id`. Earlier lines carry the initial `message_start` usage (`output_tokens = 1..8`), while the **final line** for that `message.id` carries the true `message_delta` `output_tokens`. Keeping the first line undercounts output tokens by ~98%; summing all lines overcounts input/cache tokens by **>2×** (13,788 duplicate lines across 13,370 turns).
   - **`api-request-blob` wire records:** Newer Claude Code transcripts include `type: "api-request-blob"` lines that echo `tool_use` and `tool_result` blocks. Filtering strictly to `type in ("assistant", "user")` removes ~8% phantom tool calls that appeared in naive scans.
