# efficient-ai

**Same quality, fewer tokens.**

`efficient-ai` is an open research project and toolkit for cutting the token and API cost of AI coding agents—with **Claude Code**, **Codex CLI**, and **Gemini CLI** supported as first-class targets from day one.

---

## Why This Exists

Popular "token killer" wrappers claim **60–90% token savings** by counting bytes stripped from a single command's `stdout`. When independent benchmarks test them across full agent sessions, those savings vanish—or reverse (**+7.6% higher cost** at low effort):

1. **Wrong metric:** A session's cost is driven by **re-sent conversation history across $T$ turns** (input tokens grow roughly quadratically in $T$) and **prompt-cache hit rates**, not one tool's raw byte count.
2. **Dilution:** Build, test, and `git` output make up only **~6–7%** of tool-output tokens in typical coding sessions, while **file reading is ~64%**.
3. **Recovery loops:** Lossy filtering drops the one line the agent needed. In a session with mean context $C_t \approx 193\text{k}$ tokens, **a single extra recovery turn costs ~21,300 cost-weighted tokens—wiping out the savings of 4–6 compressed commands** ([Experiment 006](experiments/006-squeeze-retrievable-compression/RESULTS.md)).
4. **Prompt-cache busting:** Naive per-turn sliding-window pruning mutates the prefix on every turn, converting `0.1×` cache reads into `1.25×` cache writes and **increasing Claude API bills by +9.2% to +111.8%** ([Experiment 007](experiments/007-cache-aware-observation-masking/RESULTS.md)).

**Our approach:** Measure **total session tokens and billed cost** across full trajectories, target the largest token drivers first, respect prompt-cache boundaries, and make every compression **lossless by default** (spill full output to `.eai/logs/last.log` with a one-line pointer so recovery is a cheap lookup, never a re-run).

---

## Technique Leaderboard (Experiments 001–007)

All results below are reproducible via the scripts in [`experiments/`](experiments/) and tools in [`tools/`](tools/):

| Technique / Tool | Layer | Stream / Component Impact | Full-Session / Corpus Impact | Quality Risk | Experiment |
|---|---|---|---|---|---|
| **[`outline`](tools/outline/) (Structure-aware file & symbol reader)** | Reads (~64% of tool tokens) | **−89.7%** on file explore; **−72.7%** on explore + symbol read | **−37.9% to −47.2%** of all read tokens (**−19.8% to −25.2%** of all tool exposure; `118.1M` fewer re-sent tokens) | Low (lossless `--symbol` & `--lines`) | [Exp 005](experiments/005-outline-read-layer/RESULTS.md) |
| **Cache-Aware Batch Observation Masking ($K=10\text{–}15, M=6$)** | History | Masks old tool outputs $>6$ turns old only at fixed $K$-turn boundaries | **−22.9% to −38.2%** raw input tokens; **−9.6% to −10.5% Claude cost**; **−21.0% to −34.8% Codex cost** (vs. **+9.2% to +111.8% cost increase** for naive per-turn masking) | Low | [Exp 007](experiments/007-cache-aware-observation-masking/RESULTS.md) |
| **Static-Prefix Diet ([`tokscope audit`](tools/tokscope/) + [`prompts/AGENTS.md`](prompts/AGENTS.md))** | Static Prefix | **−85.0%** instruction file; **−64.5%** skill idle metadata; **−65.0%** MCP schema | **−67.9% idle prefix** (`9,438 → 3,027` tok/turn), saving **320,550 raw tokens** (`39,428` cost-weighted) per 50-turn session | None | [Exp 004](experiments/004-free-wins-and-static-diet/RESULTS.md) |
| **[`squeeze`](tools/squeeze/) (Lossless spill-to-file + H8 flags + H7 loop detector)** | Observations | **−89.7%** `pytest`; **−92.3%** `cargo test`; **−96.4%** ANSI/retry logs | **−92.6% stream tokens** with **100% error/warning preservation** + `.eai/logs/last.log` pointer (zero recovery loops) | None (retrievable) | [Exp 006](experiments/006-squeeze-retrievable-compression/RESULTS.md) |
| **Compact CLI Flags (`-sb`, `--oneline`, `-q`, [`hooks/`](hooks/))** | Observations | **−67.1%** `git status`; **−72.6%** `git log`; **−96.8%** `pytest` | Free win via [`hooks/claude_pre_tool.py`](hooks/claude_pre_tool.py) & `squeeze --rewrite` | None | [Exp 004](experiments/004-free-wins-and-static-diet/RESULTS.md) |
| **Prose-Only Brevity ([`prompts/policies/h6-prose-brevity.md`](prompts/policies/h6-prose-brevity.md))** | Output | **−65.3%** assistant reply prose (no tool recaps or echoed code) | Captures **78.5% of "caveman" savings** with complete grammar & code precision intact | Low | [Exp 004](experiments/004-free-wins-and-static-diet/RESULTS.md) |
| **Structured Data Encoding (Minified JSON / Tabular CSV/TOON)** | Observations / MCP | **−38.0%** (minified JSON) to **−61.0%** (CSV) vs. pretty JSON | Direct reduction on structured tool & MCP payloads | None | [Exp 001](experiments/001-tokenizer-baselines/RESULTS.md) |
| **Code Indentation Reformatting (4 spaces → 2 spaces / tabs)** | Code Reads/Writes | **0.0%** (2 spaces); **+1.9% worse** (tabs) | **Myth busted:** modern BPE merges space runs; never reformat code to save tokens | — | [Exp 001](experiments/001-tokenizer-baselines/RESULTS.md) |

---

## Key Empirical Findings (315 Real Sessions, 13,370 API Turns, 2.59B Tokens)

### 1. Each new token is re-sent 54.4×; cache reads are 66.7% of billed cost 🧪

Using [`tokscope`](tools/tokscope/) across **315 real Claude Code transcripts (66 parent sessions + 249 sub-agent runs, 13,370 deduplicated API turns, 2.59B total tokens)** in [`experiments/003-session-token-economics`](experiments/003-session-token-economics/RESULTS.md):

| Component | Raw Tokens | Share of Raw | Cost-Weighted Tokens (`1×` = base input) | Share of Billed Cost |
|---|---:|---:|---:|---:|
| **Cache read (`0.1×`)** | 2,532,482,885 | 97.78% | 253,248,288 | **66.7%** |
| **Cache write (`1.25×` 5m / `2.0×` 1h)** | 47,374,972 | 1.83% | 76,264,021 | **20.1%** |
| **Output + thinking (`5.0×`)** | 10,012,669 | 0.39% | 50,063,345 | **13.2%** |
| **Uncached input (`1.0×`)** | 30,258 | 0.001% | 30,258 | **0.01%** |
| **Total** | **2,589,900,784** | **100.0%** | **379,605,912** | **100.0%** |

- **Sub-agents are 31.5% of total cost:** Sub-agent trajectories (`isSidechain: true`) accounted for **45.3% of turns**, **49.2% of cache writes**, and **31.5% of total cost**—and the summaries they return to parent orchestrator sessions are re-sent **233×** on average.

### 2. File reading—not build/test output—drives ~64% of tool tokens (and 57% of re-sent exposure) 🧪

| Source | Calls | First-Occurrence Share ($1\times$) | Re-sent History Share ($\sum (T-t)\cdot\text{tok}$) | Avg Amplification |
|---|---:|---:|---:|---:|
| **File reading (`Read` tool)** | 1,527 | **41.4%** | **32.0%** | 47.9× |
| **File reading via shell (`cat`, `sed`, `head`)** | 1,747 | **22.5%** | **25.0%** | 69.1× |
| **Search (`grep`/`rg`/`find`/`ls` + `Grep`/`Glob`)** | 2,011 | **9.0%** | **9.3%** | 64.3× |
| **MCP tools** | 1,330 | **7.9%** | **7.6%** | 60.0× |
| **Sub-agent returned results** | 261 | **0.9%** | **3.4%** | **233.0×** |
| **Build / test / VCS (`git`, `npm`, `node`, `python`)** | 2,336 | **~6.8%** | **~7.0%** | 64.2× |

---

## 1-Command Install (Plugin, Extension, or Skills)

`efficient-ai` ships as a native plugin/extension and skill pack (**118 idle tokens/turn total** across all 3 skills) plus a zero-dependency unified CLI (`scripts/eai.py`) that automatically accelerates with compiled Rust binaries when available.

### Option A: Claude Code Plugin (Skills +Slash Commands + `PreToolUse` Hooks)
Inside Claude Code, run:
```text
/plugin marketplace add hatimmakki/efficient-ai
/plugin install efficient-ai@efficient-ai
```
This installs:
- **Skills:** `efficient-coding`, `outline`, `tokscope`
- **Slash commands:** `/tokscope`, `/prefix-audit`, `/outline`
- **`PreToolUse` Hook:** Automatically rewrites verbose CLI calls (`git status` → `git status -sb`, `pytest` → `pytest -q --tb=short`, `cargo test` → `cargo test -q`) and nudges on whole-file reads $>350$ lines.

### Option B: Universal Agent Skills (`Claude Code`, `Codex CLI`, `Gemini CLI`, `Cursor`)
```bash
npx skills add hatimmakki/efficient-ai
```

### Option C: Gemini CLI Extension
```bash
gemini extensions install https://github.com/hatimmakki/efficient-ai
```

### Option D: Zero-Dependency Unified CLI (`scripts/eai.py`)
No virtualenv, `pip`, or Rust build required—works out of the box with standard Python 3.10+:
```bash
git clone https://github.com/hatimmakki/efficient-ai.git
cd efficient-ai

python scripts/eai.py analyze                  # Profile your ~/.claude/projects sessions
python scripts/eai.py audit . --include-user   # Audit AGENTS.md, CLAUDE.md, skills, & MCP schemas
python scripts/eai.py outline path/to/file.py  # Structure-aware symbol outline [Lstart-Lend]
python scripts/eai.py setup --project /my/repo # Copy lean AGENTS.md (227 tok) + CLAUDE.md shim
```

---

## Native Rust Acceleration (Optional)

For sub-millisecond AST/symbol outlines and stream compression, build the zero-dependency Rust binaries (`scripts/eai.py` will automatically detect and use them):

### 1. `outline` — Structure-Aware File Reader & Enclosing-Symbol Search (Rust)

```bash
cargo build --release --manifest-path tools/outline/Cargo.toml

# Inspect all symbols & [Lstart-Lend] spans (−89.7% tokens vs whole-file Read)
./tools/outline/target/release/outline tools/tokscope/tokscope/parsers.py

# Read only a specific function or method (−72.7% tokens)
./tools/outline/target/release/outline tools/tokscope/tokscope/parsers.py --symbol parse_claude_jsonl

# Search across files and resolve matches to their enclosing symbol
./tools/outline/target/release/outline search "cache_write" tools/
```

### 2. `squeeze` — Lossless Retrievable Command Wrapper & Loop Detector (Rust)

```bash
cargo build --release --manifest-path tools/squeeze/Cargo.toml

# Wrap test/build commands (strips passing noise, keeps 100% of errors, spills raw log to .eai/logs/last.log)
./tools/squeeze/target/release/squeeze -- pytest -v

# Install portable PATH shims for Claude Code, Codex CLI, and Gemini CLI
./tools/squeeze/target/release/squeeze --install-shims .eai/shims
```

---

## Repository Structure & Status

| Phase | Deliverable | Focus | Status |
|---|---|---|---|
| **0. Research & Instrument** | [`docs/research/`](docs/research/), [`tools/tokscope`](tools/tokscope/), [`experiments/001`](experiments/001-tokenizer-baselines/RESULTS.md)–[`003`](experiments/003-session-token-economics/RESULTS.md) | Literature review, cost model, full-session token/cache attribution, cross-agent fixtures | **Done** ✅ |
| **1. Free Wins** | [`prompts/`](prompts/README.md), [`experiments/004`](experiments/004-free-wins-and-static-diet/RESULTS.md) | Static-prefix diet (**H5**), prompt-cache hygiene, compact CLI flags (**H8**), prose-only brevity (**H6**) | **Done** ✅ |
| **2. Read Layer** | [`tools/outline`](tools/outline/README.md), [`experiments/005`](experiments/005-outline-read-layer/RESULTS.md) | Rust CLI for symbol outlines, targeted symbol/range reads, and enclosing-symbol search (**H4, H3**) | **Done** ✅ |
| **3. Observation Layer** | [`tools/squeeze`](tools/squeeze/README.md), [`hooks/`](hooks/README.md), [`experiments/006`](experiments/006-squeeze-retrievable-compression/RESULTS.md) | Rust CLI for lossless spill-to-file compression (**H2**), flag rewrites (**H8**), and loop detection (**H7**) | **Done** ✅ |
| **4. History Layer** | [`experiments/007`](experiments/007-cache-aware-observation-masking/RESULTS.md) | Cache-aware batch observation masking at fixed turn boundaries (**H1**) | **Done** ✅ |
| **5. Public Leaderboard** | [`data/public/`](data/public/README.md), [`CONTRIBUTING.md`](CONTRIBUTING.md) | Sanitized 315-session dataset & end-to-end technique leaderboard | **Done** ✅ |

## License

[Apache-2.0](LICENSE)
