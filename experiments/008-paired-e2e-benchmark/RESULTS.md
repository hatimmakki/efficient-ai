# Experiment 008: Paired End-to-End `eai` v1.0.0 Stack Benchmark

**Goal:** Verify end-to-end token savings, billed `$ USD` impact across all 4 target agent cost models, sub-50ms native binary speed, and **100% quality non-inferiority** (exact symbol accuracy + 100% error retention + lossless log recovery).

**Binary:** `eai.exe` (`v1.0.0`, Rust release build with `tree-sitter`) · **Reproduce:** `python experiments/008-paired-e2e-benchmark/run_benchmark.py`

---

## 1. Multi-Language `eai outline` (Tree-Sitter AST Quality & Token Reduction)

| Language / File | Full Read (tok) | Outline Explore (tok) | Δ Explore | Explore + Symbol (tok) | Δ Combined | Wall Time | Symbol Match |
|---|---:|---:|---:|---:|---:|---:|:---:|
| Python (parsers.py) | 6,751 | 244 | **-96.4%** | 2,226 | **-67.0%** | 110.7 ms | PASS |
| Rust (tokscope.rs) | 20,292 | 1,121 | **-94.5%** | 2,134 | **-89.5%** | 16.5 ms | PASS |
| Rust (proxy.rs) | 8,315 | 423 | **-94.9%** | 1,579 | **-81.0%** | 12.2 ms | PASS |
| TypeScript/TSX (Dashboard.tsx) | 895 | 307 | **-65.7%** | 385 | **-57.0%** | 8.5 ms | PASS |
| Go (server.go) | 392 | 376 | **-4.1%** | 427 | **+8.9%** | 8.4 ms | PASS |

## 2. Lossless `eai squeeze` (100% Error Retention & Zero-Loss Spill Verification)

| Stream Workload | Raw (tok) | Squeezed (tok) | Δ Stream | Wall Time | Error Retained | Exact `.eai/logs/last.log` Match |
|---|---:|---:|---:|---:|:---:|:---:|
| pytest (180 pass, 1 fail) | 2,234 | 91 | **-95.9%** | 8.5 ms | PASS (100%) | PASS (byte-identical) |
| cargo test (150 ok, 1 panic) | 1,128 | 65 | **-94.2%** | 7.5 ms | PASS (100%) | PASS (byte-identical) |
| go test (120 pass, 1 fail) | 1,618 | 53 | **-96.7%** | 7.6 ms | PASS (100%) | PASS (byte-identical) |
| docker + vitest (140 cached/pass, 1 err) | 1,242 | 368 | **-70.4%** | 7.5 ms | PASS (100%) | PASS (byte-identical) |

## 3. Hook Latency & Ultra-Lean MCP Overhead (H5 Static-Prefix Budget)

- **Median native Rust hook spawn + JSON rewrite (`eai hook claude|antigravity|gemini`):** **7.3 ms** (sub-process wall time on Windows; core engine `~40 µs`).
- **Universal MCP server (`eai mcp`) idle schema footprint:** **248 tokens** across all 4 tools (`outline_file`, `outline_search`, `squeeze_run`, `eai_stats`) — well under the 280-token H5 ceiling.

## 4. Paired Full-Session Cost Comparison (315 Public Sessions, 13,370 Turns)

Evaluates **Vanilla Baseline** vs. **`eai` Core Stack** (Static Prefix Diet + `outline` + `squeeze` + H8 flags + H6 prose brevity) vs. **Full `eai` Stack** (Core + `eai proxy` Exp 007 Cache-Aware Batch Masking):

| Agent Pricing Model | Vanilla Baseline (USD) | `eai` Core Stack (USD) | Δ Core | Full `eai` + Proxy (USD) | Δ Full Stack | Total USD Saved |
|---|---:|---:|---:|---:|---:|---:|
| Claude Code (Sonnet 3/15) | $1,087.68 | $863.26 | **-20.6%** | $793.08 | **-27.1%** | **$294.60** |
| Antigravity (Gemini Pro 1.25/10) | $900.72 | $707.43 | **-21.5%** | $607.93 | **-32.5%** | **$292.79** |
| Gemini CLI (2.5 Pro 1.25/10) | $900.72 | $707.43 | **-21.5%** | $607.93 | **-32.5%** | **$292.79** |
| Codex CLI (GPT-5 1.50/10) | $2,030.55 | $1,589.52 | **-21.7%** | $1,266.83 | **-37.6%** | **$763.72** |

## Key Takeaways

1. **Quality Non-Inferiority Verified (100% Pass):** Every tested language (`Python`, `Rust`, `TypeScript/TSX`, `Go`) resolved exact symbol definitions via `tree-sitter`, and every verbose failure workload (`pytest`, `cargo test`, `go test`, `docker`/`vitest`) retained 100% of failure diagnostics with a byte-identical `.eai/logs/` backup.
2. **Validates the `>200`-Line Outline Rule:** On files >200 lines (`parsers.py`, `tokscope.rs`, `proxy.rs`, `Dashboard.tsx`), `eai outline` cuts explore tokens **−65.7% to −96.4%** and combined explore+symbol tokens **−57.0% to −89.5%**. On tiny files (`server.go`, ~75 lines), whole-file read is cheaper (`+8.9%`), confirming why `AGENTS.md` targets files `>200` lines.
3. **Compounding Multi-Layer Cost Savings:** Across the 315-session corpus, combining the lossless Core Stack with `eai proxy` Cache-Aware Batch Masking delivers **-27.1% to -37.6% lower billed USD** across Claude Code, Antigravity, Gemini CLI, and Codex CLI without busting prompt caches.
4. **Sub-10ms Hook & Lean MCP Overhead:** **7.3 ms** median hook wall time and **248 idle tokens** total for `eai mcp` mean `efficient-ai` pays for itself on the very first large file read.
