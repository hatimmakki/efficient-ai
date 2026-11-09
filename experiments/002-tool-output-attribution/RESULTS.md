# Experiment 002: Tool-output attribution in real Claude Code sessions

**Question:** Which tools produce the tokens that agents ingest? This tells us which tool to build first.

**Method:** [`analyze.py`](analyze.py) scans local Claude Code transcripts and attributes every tool result to a category. Shell commands are split by program name (after stripping `cd <dir> &&` prefixes).
- Token counts use tiktoken `o200k_base` as a proxy.
- Each output is counted **once**. Re-sending through history multiplies all categories, and isn't modeled here.

**Data:** 317 sessions from one developer's machine, mixed projects, 2026. Only aggregates are published.

**Run:** 2026-11-01

## Results

**8.05M tool-output tokens in total.**

| category | calls | tokens | share | tokens/call |
|---|---:|---:|---:|---:|
| Read | 1,660 | 3,435,443 | 42.7% | 2,069 |
| Bash (other) | 5,883 | 2,790,676 | 34.7% | 474 |
| MCP | 1,603 | 664,958 | 8.3% | 414 |
| Search (shell: grep/rg/find/ls) | 1,739 | 565,780 | 7.0% | 325 |
| Search (Grep/Glob tools) | 353 | 132,466 | 1.6% | 375 |
| WebSearch | 157 | 98,670 | 1.2% | 628 |
| PowerShell (other) | 330 | 88,721 | 1.1% | 268 |
| Edit | 1,854 | 80,487 | 1.0% | 43 |
| Agent (sub-agent results) | 282 | 71,000 | 0.9% | 251 |
| WebFetch | 284 | 66,972 | 0.8% | 235 |

**Shell output by program** (top 12):

| program | calls | tokens | share of shell | tokens/call |
|---|---:|---:|---:|---:|
| cat | 770 | 980,506 | 28.5% | 1,273 |
| sed | 961 | 712,381 | 20.7% | 741 |
| grep | 1,352 | 420,568 | 12.2% | 311 |
| git | 959 | 278,342 | 8.1% | 290 |
| other | 1,020 | 271,977 | 7.9% | 266 |
| echo | 305 | 185,150 | 5.4% | 607 |
| ls | 282 | 123,409 | 3.6% | 437 |
| npm | 437 | 113,284 | 3.3% | 259 |
| node | 415 | 72,245 | 2.1% | 174 |
| wc | 110 | 59,533 | 1.7% | 541 |
| python | 575 | 58,146 | 1.7% | 101 |

**Read tool:**
- 1,663 calls: 1,035 whole-file (62%), 628 ranged.
- Tokens per read: median 1,164, p90 5,592, p99 12,276, max 16,922.
- **The largest 10% of reads make up 40.9% of read tokens.**
- 273 reads came immediately after a search call.

## Findings

1. **File reading is the dominant cost: ~64% of all tool-output tokens.** That's the `Read` tool (3.44M) plus file reading through the shell: `cat` (0.98M) and `sed -n` ranges (0.71M). **The first tool to build after measurement is a smarter reader** (`outline`: skim/outline first, symbol and range reads).
2. **Reads have a heavy tail.** One read in ten carries 41% of read tokens. A rule like "outline instead of whole-file for files over ~5k tokens" targets a large share while touching few calls.
3. **Search is ~9%** (shell grep/find/ls 7.0% + Grep/Glob 1.6%). A search command that only shortens results has limited upside. A search that **returns the enclosing symbol** and avoids the follow-up read is worth folding into `outline`.
4. **Build/test/git output is small here.** `git` + `npm` + `node` + `python` together are ~15% of shell output, about 6% of all tool output. This is consistent with RTK's lack of end-to-end savings: it compresses the wrong stream for this kind of workload. `squeeze` should be **reprioritized below `outline`**. Test-heavy workflows may differ, so re-check with more users.
5. **MCP is 8.3%:** a worthwhile target for `slim-mcp` later.

## Caveats

- One developer, one agent (Claude Code). Must be reproduced on Codex / Gemini CLI and on other people's sessions. The script takes the transcripts root as an argument, and contributions of aggregate tables are welcome.
- These are first-occurrence tokens. Their real cost depends on how many later turns re-send them, and on cache pricing. `tokscope` will model that.
- `o200k_base` is a proxy for Claude's tokenizer.

