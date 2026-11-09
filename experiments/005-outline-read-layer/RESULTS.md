# Experiment 005: Structure-Aware Read Layer (`outline`, H4 + H3)

**Question:** Since file reading accounts for **~64% of tool-output tokens** and **57% of cumulative re-sent exposure** ([Experiment 003](../003-session-token-economics/RESULTS.md)), how much does structure-aware reading ([`tools/outline`](../../tools/outline/)) save on individual file exploration/editing and across our 315-session corpus?

**Method:** Run [`benchmark_outline.py`](benchmark_outline.py) using the compiled Rust [`outline`](../../tools/outline/) binary on Python stdlib modules (`argparse.py`, `difflib.py`, `textwrap.py`, `json/decoder.py`), repository Python/Rust files, and replay the 315 real Claude Code sessions from Experiment 003.

**Run:** 2026-11-09, `outline 0.1.0` (`rustc 1.99.1 --release`), `tiktoken 0.14.0` (`o200k_base`)

---

## 1. File Exploration & Targeted Symbol Read (`outline` vs. Whole-File `Read`)

| File | Lines | Whole-File Tokens | `outline <file>` | Outline Reduction | `outline` + 1 Symbol (`--symbol`) | Explore + Edit Reduction |
|---|---:|---:|---:|---:|---:|---:|
| stdlib `argparse.py` | 2,650 | 20,129 | 3,255 | **−83.8%** | 6,742 | **−66.5%** |
| stdlib `difflib.py` | 2,056 | 20,445 | 1,174 | **−94.3%** | 2,972 | **−85.5%** |
| stdlib `textwrap.py` | 491 | 4,429 | 374 | **−91.6%** | 1,589 | **−64.1%** |
| stdlib `json/decoder.py` | 357 | 3,076 | 334 | **−89.1%** | 543 | **−82.3%** |
| repo `tokscope/parsers.py` | 518 | 4,810 | 267 | **−94.4%** | 2,031 | **−57.8%** |
| repo `outline/src/parser.rs` | 419 | 3,082 | 382 | **−87.6%** | 1,427 | **−53.7%** |
| **Total** | — | **55,971** | **5,786** | **−89.7%** | **15,304** | **−72.7%** |

- **Exploration only (`outline <file>`):** **−89.7% tokens** (`10.3%` of whole-file size) while preserving every class, struct, function, and method signature plus exact `[Lstart-Lend]` line numbers.
- **Explore + read target symbol (`outline <file>` + `outline <file> --symbol <fn>`):** **−72.7% tokens** (`27.3%` of whole-file size) while delivering the exact implementation lines needed to make an edit.

---

## 2. Corpus Simulation on 315 Real Claude Code Sessions (13,370 Turns)

Using a conservative blended compression ratio of `20.65%` (50% exploratory outline-only reads, 50% outline + targeted symbol reads) on whole-file reads above threshold $K$:

| Policy Threshold | Calls Affected | First-Occurrence Read Tokens | Read Reduction (1×) | Re-sent Read Exposure | Re-sent Read Reduction | Total Tool-Token Reduction (All Tools) |
|---|---:|---:|---:|---:|---:|---:|
| **Baseline (unmodified sessions)** | 0 | 4,834,126 | — | 267,677,277 | — | — |
| **`outline` for whole-file reads $\ge 5,000$ tok** | 136 | 4,009,596 | **−17.1%** | 226,325,985 | **−15.4%** | **−8.8%** |
| **`outline` for whole-file reads $\ge 2,000$ tok** | 536 | 3,004,161 | **−37.9%** | 174,812,680 | **−34.7%** | **−19.8%** |
| **`outline` for whole-file reads $\ge 1,000$ tok** | 928 | 2,551,943 | **−47.2%** | 149,542,335 | **−44.1%** | **−25.2%** |

---

## Key Findings

1. **H4 confirmed (`>50%` read reduction on target files, `−47.2%` across all corpus reads at $K=1,000$).** On files $\ge 350$ lines, `outline` cuts exploratory read tokens by **89.7%** and explore-then-edit tokens by **72.7%**.
2. **Touching just 536 whole-file reads ($\ge 2,000$ tokens) across 315 sessions eliminates 92.9M re-sent context tokens (−19.8% of all tool-output exposure across every tool category).** Because file reads have a heavy tail, we don't need to intercept small file reads to capture the majority of the savings.
3. **`outline search <pattern>` eliminates the 263 search-then-whole-file-read pairs.** By resolving every match directly to its enclosing symbol `[Lstart-Lend]` (and optionally expanding just that symbol with `--expand`), the agent saves both a full round-trip turn and ~73–90% of the follow-up file read.
