"""Experiment 001 - tokenizer baselines.

Question: how much do *representation choices* (indentation, JSON formatting,
tabular encodings, comment stripping) change token counts, before we touch any
agent behaviour?

Uses OpenAI's public BPE tokenizers (o200k_base, cl100k_base) as proxies.
Anthropic and Google tokenizers are not public; validate any promising result
with the provider's count-tokens endpoint before relying on it.

Inputs are taken from the local Python stdlib so the experiment is fully
reproducible and contains no private data.

Usage:  python measure.py            (prints a markdown table)
"""

from __future__ import annotations

import io
import json
import re
import sys
import sysconfig
import tokenize
from pathlib import Path

import tiktoken

ENCODINGS = ["o200k_base", "cl100k_base"]
STDLIB = Path(sysconfig.get_paths()["stdlib"])
CODE_FILES = ["argparse.py", "json/decoder.py", "textwrap.py", "difflib.py"]


def count(enc: tiktoken.Encoding, text: str) -> int:
    return len(enc.encode(text, disallowed_special=()))


# ---------------------------------------------------------------- code variants
def reindent(src: str, unit: str) -> str:
    """Re-express leading 4-space indentation with another unit."""
    out = []
    for line in src.splitlines(keepends=True):
        stripped = line.lstrip(" ")
        n = len(line) - len(stripped)
        out.append(unit * (n // 4) + " " * (n % 4) + stripped)
    return "".join(out)


def strip_comments_and_docstrings(src: str) -> str:
    """Remove comments and docstrings using the tokenizer (keeps code valid)."""
    out_tokens = []
    prev_type = tokenize.INDENT
    toks = tokenize.generate_tokens(io.StringIO(src).readline)
    for tok in toks:
        if tok.type == tokenize.COMMENT:
            continue
        if tok.type == tokenize.STRING and prev_type in (
            tokenize.INDENT,
            tokenize.NEWLINE,
            tokenize.DEDENT,
        ):
            prev_type = tok.type
            continue  # bare string statement == docstring
        out_tokens.append(tok)
        if tok.type not in (tokenize.NL, tokenize.COMMENT):
            prev_type = tok.type
    text = tokenize.untokenize(out_tokens)
    return re.sub(r"\n\s*\n+", "\n", text)


def blank_line_collapse(src: str) -> str:
    return re.sub(r"\n\s*\n+", "\n", src)


# ---------------------------------------------------------------- data variants
def make_records(n: int = 60) -> list[dict]:
    return [
        {
            "id": i,
            "name": f"package-{i}",
            "version": f"{i % 7}.{i % 13}.{i % 5}",
            "license": ["MIT", "Apache-2.0", "BSD-3-Clause"][i % 3],
            "outdated": i % 4 == 0,
        }
        for i in range(n)
    ]


def as_csv_like(records: list[dict]) -> str:
    keys = list(records[0])
    lines = [",".join(keys)]
    lines += [",".join(str(r[k]) for k in keys) for r in records]
    return "\n".join(lines)


def as_toon_like(records: list[dict]) -> str:
    """Approximation of TOON's tabular array form: items[N]{k1,k2}: rows."""
    keys = list(records[0])
    head = f"items[{len(records)}]{{{','.join(keys)}}}:"
    rows = ["  " + ",".join(str(r[k]).lower() if isinstance(r[k], bool) else str(r[k]) for k in keys) for r in records]
    return "\n".join([head, *rows])


def as_yaml_like(records: list[dict]) -> str:
    out = []
    for r in records:
        first = True
        for k, v in r.items():
            v = str(v).lower() if isinstance(v, bool) else v
            out.append(f"{'- ' if first else '  '}{k}: {v}")
            first = False
    return "\n".join(out)


# ---------------------------------------------------------------- prose variants
VERBOSE_REPLY = (
    "Great question! I'd be happy to help you with that. After carefully "
    "reviewing the code, I can see that the issue is caused by the fact that "
    "the `parse_args` function is being called before the configuration file "
    "has been loaded. This means that the default values are being used "
    "instead of the values from the configuration file. To fix this, you "
    "should move the call to `load_config()` so that it happens before "
    "`parse_args()` is invoked. I've gone ahead and made that change for you "
    "in `cli.py`. Let me know if you have any other questions or if there's "
    "anything else I can help you with!"
)
TERSE_REPLY = (
    "Cause: `parse_args()` runs before `load_config()`, so defaults win. "
    "Fixed in `cli.py`: moved `load_config()` first."
)


def main() -> None:
    encs = {name: tiktoken.get_encoding(name) for name in ENCODINGS}
    rows: list[tuple[str, str, dict[str, int]]] = []

    def add(group: str, label: str, text: str) -> None:
        rows.append((group, label, {n: count(e, text) for n, e in encs.items()}))

    code = "".join((STDLIB / f).read_text(encoding="utf-8") for f in CODE_FILES)
    add("code", "original (4 spaces)", code)
    add("code", "2-space indent", reindent(code, "  "))
    add("code", "tab indent", reindent(code, "\t"))
    add("code", "blank lines collapsed", blank_line_collapse(code))
    add("code", "comments+docstrings stripped", strip_comments_and_docstrings(code))

    recs = make_records()
    add("data", "JSON pretty (indent=2)", json.dumps(recs, indent=2))
    add("data", "JSON minified", json.dumps(recs, separators=(",", ":")))
    add("data", "YAML-like", as_yaml_like(recs))
    add("data", "TOON-like tabular", as_toon_like(recs))
    add("data", "CSV", as_csv_like(recs))

    add("prose", "verbose assistant reply", VERBOSE_REPLY)
    add("prose", "terse assistant reply", TERSE_REPLY)

    # markdown table, % relative to first row of each group
    base: dict[str, dict[str, int]] = {}
    hdr = "| group | variant | " + " | ".join(ENCODINGS) + " |"
    print(hdr)
    print("|" + "---|" * (2 + len(ENCODINGS)))
    for group, label, counts in rows:
        base.setdefault(group, counts)
        cells = []
        for n in ENCODINGS:
            pct = 100 * (counts[n] - base[group][n]) / base[group][n]
            cells.append(f"{counts[n]:,} ({pct:+.1f}%)" if pct else f"{counts[n]:,}")
        print(f"| {group} | {label} | " + " | ".join(cells) + " |")
    print(f"\nPython {sys.version.split()[0]}, tiktoken {tiktoken.__version__}")


if __name__ == "__main__":
    main()

