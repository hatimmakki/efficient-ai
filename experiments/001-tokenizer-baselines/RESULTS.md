# Experiment 001: Tokenizer baselines

**Question:** Before we change any agent behaviour, how much do *representation choices* change token counts?

**Method:** Run [`measure.py`](measure.py) on Python stdlib sources (`argparse.py`, `json/decoder.py`, `textwrap.py`, `difflib.py`), a synthetic 60-row table, and a pair of assistant replies. We use OpenAI's public BPE tokenizers as stand-ins. **Anthropic/Google tokenizers aren't public**, so anything that looks promising here must be re-checked with the provider's count-tokens API.

**Run:** 2026-11-01, Python 3.12.10, tiktoken 0.14.0

| group | variant | o200k_base | cl100k_base |
|---|---|---|---|
| code | original (4 spaces) | 48,079 | 47,987 |
| code | 2-space indent | 48,079 | 47,987 |
| code | tab indent | 48,983 (+1.9%) | 48,851 (+1.8%) |
| code | blank lines collapsed | 48,053 (-0.1%) | 47,962 (-0.1%) |
| code | comments+docstrings stripped | 28,167 (-41.4%) | 28,012 (-41.6%) |
| data | JSON pretty (indent=2) | 2,842 | 2,842 |
| data | JSON minified | 1,762 (-38.0%) | 1,762 (-38.0%) |
| data | YAML-like | 2,119 (-25.4%) | 2,119 (-25.4%) |
| data | TOON-like tabular | 1,173 (-58.7%) | 1,173 (-58.7%) |
| data | CSV | 1,108 (-61.0%) | 1,048 (-63.1%) |
| prose | verbose assistant reply | 125 | 128 |
| prose | terse assistant reply | 30 (-76.0%) | 30 (-76.6%) |

## Findings

1. **Indentation width and blank lines don't matter. Tabs make things *worse*.** Modern BPE vocabularies merge a run of spaces into a single token, so 2-space and 4-space code cost exactly the same. Tabs cost about 2% *more*. This goes against a claim that circulates widely online ("tabs save tokens"). **Takeaway: don't reformat code to save tokens.**
2. **Comments and docstrings are the big lever in code (~41% of stdlib tokens).** But they often carry the intent an agent needs, so this is a *quality* question. It's not a free win. Candidate use: *outline/skim* views (signatures only) for files the agent is exploring, not files it is editing.
3. **Structured data: the encoding matters a lot.** Minified JSON is −38% versus pretty JSON. Tabular encodings (CSV/TOON) are about −60%. This applies to tool outputs that agents ingest (`--json` flags, MCP payloads, package lists).
4. **Prose terseness cuts 76% of the prose itself.** But prose is a small share of an agent's tokens; see the literature review (JetBrains measured ~8.5% end-to-end for the "caveman" skill).

## Caveats

- These are proxy tokenizers. Claude's tokenizer in particular is known to differ.
- Token counts alone say nothing about quality. Every lever needs an end-to-end A/B (see `docs/research/00-state-of-the-art.md`, §6).

