# Experiment 006: Lossless Retrievable Compression (`squeeze`, H2 + H7 + H8)

**Question:** Can [`tools/squeeze`](../../tools/squeeze/) achieve RTK-level stream compression (**89–96%**) on build, test, and runtime logs **without** dropping error/warning signals or triggering extra-turn recovery loops?

**Method:** Run [`benchmark_squeeze.py`](benchmark_squeeze.py) using the compiled Rust [`squeeze`](../../tools/squeeze/) binary across three representative developer streams:
1. `pytest` suite (120 tests: 118 passing, 2 failing with assertion tracebacks).
2. `cargo test` build + test run (80 crates downloaded/compiled, compiler warning, 60 passing unit tests, 1 panic failure).
3. Runtime migration log with 99 `\r` ANSI progress-bar updates, 45 repeated connection retry warnings, and a fatal SQL error.

**Run:** 2026-11-09, `squeeze 0.1.0` (`rustc 1.99.1 --release`), `tiktoken 0.14.0` (`o200k_base`)

---

## 1. Stream Compression & Signal Preservation

| Workload Stream | Raw Tokens (`o200k`) | `squeeze` Tokens | Reduction | All Error/Warning Signals Preserved? | Full Log Spilled to `.eai/logs/last.log`? |
|---|---:|---:|---:|---|---|
| `pytest` (118 passed, 2 failed) | 2,601 | 268 | **−89.7%** | **Yes (100%)** | **Yes** |
| `cargo test` (80 crates, 60 ok, 1 failed) | 2,587 | 200 | **−92.3%** | **Yes (100%)** | **Yes** |
| ANSI progress + 45× repeated warn + error | 2,155 | 77 | **−96.4%** | **Yes (100%)** | **Yes** |
| **Total** | **7,343** | **545** | **−92.6%** | **Yes (100%)** | **Yes** |

---

## 2. Break-Even Math: Why Retrievable Spill-to-File Beats Lossy Filtering

Using our empirical session parameters from [Experiment 003](../003-session-token-economics/RESULTS.md) (mean context length per turn $C_t \approx 193,000$ tokens across 13,370 turns, `98.16%` cache hit rate):

- **Gain from compressing one test run (−2,300 tokens) over 10 remaining turns:**
  $$2,300 \times (1.25 + 10 \times 0.10) = \mathbf{5,175}\text{ cost-weighted tokens saved}$$
- **Cost of a single recovery turn when a lossy filter hides a needed line:**
  $$193,000 \times 0.10\text{ (cache read)} + 400 \times 5.0\text{ (output)} = \mathbf{21,300}\text{ cost-weighted tokens spent}$$
- **Result:** A single recovery turn wipes out the savings of **4.1 to 6.3 compressed commands**. That explains why independent benchmarks found lossy filters to be **+7.6% more expensive** end-to-end at low effort.
- By preserving 100% of failure/warning lines in-stream and spilling the full raw output to `.eai/logs/last.log` with a one-line pointer, `squeeze` captures **−92.6%** stream reduction while keeping full-log recovery to a single targeted read if ever needed.
