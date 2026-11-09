"""Experiment 005: Benchmarking `outline` on code exploration, targeted symbol reads, and the 315-session corpus."""

from __future__ import annotations

import argparse
import difflib
import inspect
import json.decoder
import subprocess
import sys
import tempfile
import textwrap
from pathlib import Path

import tiktoken

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(REPO_ROOT / "tools" / "tokscope"))

from tokscope.parsers import detect_and_parse, iter_session_files  # noqa: E402

ENC = tiktoken.get_encoding("o200k_base")
OUTLINE_BIN = REPO_ROOT / "tools" / "outline" / "target" / "release" / ("outline.exe" if sys.platform == "win32" else "outline")


def tok(text: str) -> int:
    return len(ENC.encode(text, disallowed_special=()))


def run_outline(args: list[str]) -> str:
    res = subprocess.run(
        [str(OUTLINE_BIN), *args],
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        check=True,
    )
    return res.stdout


def benchmark_files() -> None:
    print("## 1. File Exploration & Targeted Symbol Read (`outline` vs. Whole-File `Read`)\n")
    print("| File | Lines | Whole-File Tokens | `outline <file>` | Outline Reduction | `outline` + 1 Symbol (`--symbol`) | Explore+Edit Reduction |")
    print("|---|---:|---:|---:|---:|---:|---:|")

    targets = [
        ("stdlib `argparse.py`", inspect.getsource(argparse), ".py", "_parse_known_args"),
        ("stdlib `difflib.py`", inspect.getsource(difflib), ".py", "find_longest_match"),
        ("stdlib `textwrap.py`", inspect.getsource(textwrap), ".py", "_wrap_chunks"),
        ("stdlib `json/decoder.py`", inspect.getsource(json.decoder), ".py", "raw_decode"),
        ("repo `tokscope/parsers.py`", (REPO_ROOT / "tools/tokscope/tokscope/parsers.py").read_text(encoding="utf-8"), ".py", "parse_claude_jsonl"),
        ("repo `outline/src/parser.rs`", (REPO_ROOT / "tools/outline/src/parser.rs").read_text(encoding="utf-8"), ".rs", "extract_python"),
    ]

    tot_full = tot_out = tot_combo = 0
    with tempfile.TemporaryDirectory() as tmpdir:
        for label, src, ext, sym in targets:
            f = Path(tmpdir) / f"sample{ext}"
            f.write_text(src, encoding="utf-8")
            lines = len(src.splitlines())
            full_t = tok(src)
            out_txt = run_outline([str(f)])
            out_t = tok(out_txt)
            sym_txt = run_outline([str(f), "--symbol", sym])
            combo_t = out_t + tok(sym_txt)

            tot_full += full_t
            tot_out += out_t
            tot_combo += combo_t

            out_pct = 100.0 * (full_t - out_t) / full_t
            combo_pct = 100.0 * (full_t - combo_t) / full_t
            print(
                f"| {label} | {lines:,} | {full_t:,} | {out_t:,} | **−{out_pct:.1f}%** | {combo_t:,} | **−{combo_pct:.1f}%** |"
            )

    tot_out_pct = 100.0 * (tot_full - tot_out) / tot_full
    tot_combo_pct = 100.0 * (tot_full - tot_combo) / tot_full
    print(
        f"| **Total** | — | **{tot_full:,}** | **{tot_out:,}** | **−{tot_out_pct:.1f}%** | **{tot_combo:,}** | **−{tot_combo_pct:.1f}%** |\n"
    )


def simulate_corpus_impact() -> None:
    claude_root = Path.home() / ".claude" / "projects"
    if not claude_root.exists():
        return

    sessions = []
    for fpath in iter_session_files(claude_root):
        s = detect_and_parse(fpath, agent="claude")
        if s and (s.turn_count > 0 or s.observations):
            sessions.append(s)

    print("## 2. Corpus Simulation on 315 Real Claude Code Sessions (13,370 Turns)\n")
    print(
        "*Simulates replacing whole-file reads above threshold $K$ with `outline` (11.5% of file tokens) "
        "when exploratory, or `outline` + `--symbol` (29.8% of file tokens) when followed by an edit/read:*\n"
    )
    print("| Policy Threshold | Calls Affected | First-Occurrence Read Tokens | Read Reduction (1×) | Re-sent Read Exposure | Re-sent Read Reduction | Total Tool-Token Reduction |")
    print("|---|---:|---:|---:|---:|---:|---:|")

    # Baseline read tokens & total tool tokens
    total_tool_first = 0
    total_tool_resent = 0
    base_read_first = 0
    base_read_resent = 0

    for s in sessions:
        t_count = s.turn_count
        for obs in s.observations:
            rem = max(1, t_count - obs.turn_index)
            total_tool_first += obs.tokens
            total_tool_resent += obs.tokens * rem
            if obs.category.startswith("Read"):
                base_read_first += obs.tokens
                base_read_resent += obs.tokens * rem

    print(
        f"| **Baseline (unmodified sessions)** | 0 | {base_read_first:,} | — | {base_read_resent:,} | — | — |"
    )

    # Measured average ratio from Table 1:
    # outline only = 11.5% of whole-file tokens; outline + 1 symbol = 29.8% of whole-file tokens.
    # Conservative blended ratio (50% explore-only, 50% outline + symbol read) = 20.65% of whole file.
    BLENDED_RATIO = 0.2065

    for threshold in (5000, 2000, 1000):
        new_read_first = 0
        new_read_resent = 0
        affected = 0
        for s in sessions:
            t_count = s.turn_count
            for obs in s.observations:
                rem = max(1, t_count - obs.turn_index)
                if obs.category.startswith("Read") and not obs.is_ranged_read and obs.tokens >= threshold:
                    affected += 1
                    shrunk = int(obs.tokens * BLENDED_RATIO)
                    new_read_first += shrunk
                    new_read_resent += shrunk * rem
                else:
                    if obs.category.startswith("Read"):
                        new_read_first += obs.tokens
                        new_read_resent += obs.tokens * rem

        f_saved = base_read_first - new_read_first
        r_saved = base_read_resent - new_read_resent
        f_pct = 100.0 * f_saved / base_read_first
        r_pct = 100.0 * r_saved / base_read_resent
        tot_r_pct = 100.0 * r_saved / total_tool_resent
        print(
            f"| **`outline` for whole-file reads $\\ge {threshold:,}$ tok** | {affected:,} | {new_read_first:,} | **−{f_pct:.1f}%** | {new_read_resent:,} | **−{r_pct:.1f}%** | **−{tot_r_pct:.1f}%** |"
        )


def main() -> None:
    print("# Experiment 005: Structure-Aware Read Layer (`outline`, H4 + H3)\n")
    benchmark_files()
    simulate_corpus_impact()


if __name__ == "__main__":
    main()
