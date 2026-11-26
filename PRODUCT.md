# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Individual developers and engineering teams running AI coding agents—specifically **Claude Code**, **Antigravity**, **Gemini CLI**, **Codex CLI**, and **Cursor** (plus MCP-compatible agents)—who want to cut real multi-turn API token spend and context bloat without degrading code quality or breaking prompt caches.

## Product Purpose

`efficient-ai` is an Apache-2.0 open-source research project and zero-dependency unified Rust CLI (`eai`, v1.2.0) with native plugins and extensions that measures and cuts full-session AI coding agent API bills by **27.1%–37.6%** while keeping 100% lossless recovery and prompt-cache stability.

On the public website (`https://hatimmakki.github.io/efficient-ai/`), success means a visiting developer immediately understands why full-session cache economics beat naive token wrappers, verifies the real dollar impact for their agent/workload, and copies and runs the 1-click install command for their environment.

## Positioning

Unlike single-stream "token killer" wrappers that advertise raw bytes stripped from `git`/`pytest` stdout (only ~6.8% of tool tokens) and silently drop error lines—causing recovery turns that cost ~21,300 cost-weighted tokens each—and unlike naive per-turn history pruners that invalidate prompt-cache prefixes every turn (+9.2% to +111.8% higher Claude bills), `efficient-ai` targets full-session billed cost across all four amplification layers:

1. **Read layer (`eai outline`)** — tree-sitter AST outlines and targeted `--symbol` / `--lines` reads attacking the ~64% file-reading share.
2. **Observation layer (`eai squeeze`)** — lossless spill-to-file stream compression across 15+ runners keeping 100% of errors/warnings with a byte-identical `.eai/logs/` pointer and H7 3× stuck-loop guard.
3. **Static-prefix layer (`eai audit --fix`)** — minifying `AGENTS.md`, skills, and MCP schema overhead (`eai mcp` stays at 248 idle tokens across 4 tools).
4. **History layer (`eai proxy`)** — opt-in cache-aware batch observation masking at fixed $K$-turn boundaries ($K=12, M=6$) for Anthropic, OpenAI Chat/Responses, and Gemini.

## Operating Context

- Local developer shells on macOS, Linux, and Windows PowerShell.
- Native plugin marketplaces and agent hook lifecycles (`PreToolUse`, `BeforeTool`, `AfterTool`, `PostInvocation`) across Claude Code, Antigravity, Gemini CLI, Codex CLI, and Cursor/MCP.
- Offline local telemetry and diagnostics (`eai analyze`, `eai impact`, `eai dashboard` on `127.0.0.1:7420`, `eai watch`, `eai share`, `eai doctor`, `eai pause`/`resume`, `eai uninstall`).
- Reproducible open-source benchmarks under `experiments/001` through `experiments/009` and `data/public/`.

## Capabilities and Constraints

- **Deployment:** Plain static HTML/CSS/JS deployed to GitHub Pages via `.github/workflows/pages.yml` (`docs/index.html` staged into `_site/`). Any local assets referenced by the page must be staged by that workflow.
- **Required interactive features on `docs/index.html`:**
  1. Multi-platform 1-click copy install command switcher (`macOS / Linux`, `Windows PowerShell`, `Claude Code`, `Antigravity`, `Gemini CLI`, `Codex CLI`, `Cursor / MCP`).
  2. Interactive Prompt-Cache & Token Cost Calculator calibrated to Experiments 003–008 (exact LCP cache simulation comparing Vanilla Baseline, Naive Per-Turn Pruning $M=6$, and Full `efficient-ai` Stack across Claude Sonnet, Antigravity/Gemini Pro, and Codex/GPT-5).
  3. Filterable Empirical Technique Leaderboard covering Experiments 001–008 across layers (`All`, `Reads`, `History`, `Prefix`, `Streams`, `Output`).
- **Release & privacy invariants:** Current version is `v1.2.0`; Apache-2.0 open source; 100% local execution with zero cloud telemetry or external data upload.
- **Existing SVGs:** `assets/hero-demo.svg`, `assets/architecture-flow.svg`, and `assets/sample-proof-card.svg` are not binding brand assets for the website and may be replaced or omitted to fit the redesigned page.
- **Undecided / excluded:** `packages/cli` is not yet published to npm; do not advertise `npx efficient-ai` as an active primary install command.

## Brand Commitments

- **Name & CLI:** `efficient-ai` (binary: `eai`).
- **Tagline:** "Same quality, fewer tokens."
- **Voice:** Empirical, falsifiable, engineer-to-engineer—lead with reproducible measurements and debunk industry myths rather than hype.

## Evidence on Hand

- **Public session corpus** (`data/public/`, `experiments/003-session-token-economics/RESULTS.md`): 315 real Claude Code sessions (66 parent + 249 sub-agent runs), 13,370 deduplicated API turns, 2.59B total tokens (97.78% raw / 66.7% billed cost from cache reads; 54.4× average token re-send amplification; 64% of tool tokens from file reading vs. ~6.8% from build/test/git; sub-agent summaries re-sent 233×).
- **Paired end-to-end benchmark** (`experiments/008-paired-e2e-benchmark/RESULTS.md`): Full `eai` stack cuts billed USD by **−27.1%** on Claude Code ($1,087.68 → $793.08), **−32.5%** on Antigravity & Gemini CLI ($900.72 → $607.93), and **−37.6%** on Codex CLI ($2,030.55 → $1,266.83); 7.3 ms median native hook wall time; 248 idle MCP tokens across 4 tools; 100% symbol accuracy and 100% error retention with byte-identical `.eai/logs/` backup.
- **Layer benchmarks** (`experiments/001`–`007`):
  - `eai outline`: −65.7% to −96.4% explore, −57.0% to −89.5% combined on files >200 lines; −37.9% to −47.2% read tokens (`118.1M` fewer re-sent tokens).
  - `eai proxy` Cache-Aware Batch Masking ($K=12, M=6$): −9.6% to −10.5% Claude USD, −21.0% to −34.8% Codex USD, vs. +9.2% to +111.8% higher Claude cost for naive per-turn pruning.
  - Static-Prefix Diet (`eai audit --fix`): −67.9% idle prefix (`9,438 → 3,027` tok/turn; 320,550 raw tokens saved per 50-turn session).
  - `eai squeeze`: −92.6% stream tokens (−94.2% to −96.7% on `pytest`/`cargo`/`go`), 100% error retention, zero recovery loops.
  - Prose-Only Brevity (`H6`): −65.3% assistant reply prose (78.5% of caveman savings with intact code/grammar).
  - Indentation reformatting (`Exp 001`): 0.0% for 2-space, +1.9% worse for tabs (myth busted).
- **Distribution:** GitHub Release `v1.2.0` (`https://github.com/hatimmakki/efficient-ai/releases/tag/v1.2.0`) with prebuilt binaries for macOS (arm64/x64), Linux (x64), and Windows (x64), plus `install.sh` and `install.ps1`.
- **Absences (do not fabricate):** No customer logos, third-party testimonials, press quotes, commercial pricing tiers, SaaS hosting, or unverified local-only session counts (do not carry forward the old `2,824 sessions` figure).

## Product Principles

- Measure full-session billed cost and cache amplification, never isolated single-command byte counts.
- Target token drivers in empirical priority order (file reads and static prefix before test/git streams).
- Preserve prompt-cache prefix stability first; compact history only at fixed turn boundaries.
- Keep every compression lossless and locally inspectable so agent recovery loops stay at zero.
- Make adoption instant, local-first, transparent, and cleanly reversible (`eai init`, `eai doctor`, `eai pause`, `eai uninstall`).
