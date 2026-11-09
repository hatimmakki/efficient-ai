"""CLI entry point for tokscope (`analyze`, `audit`, `sanitize`, `selftest`)."""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import asdict
from pathlib import Path

from .attribution import format_markdown_report, summarize_corpus
from .audit import audit_directory, format_audit_markdown
from .parsers import detect_and_parse, iter_session_files
from .sanitize import write_sanitized_json


def _default_claude_root() -> Path:
    return Path.home() / ".claude" / "projects"


def cmd_analyze(args: argparse.Namespace) -> int:
    root = Path(args.path) if args.path else _default_claude_root()
    if not root.exists():
        print(f"Error: path does not exist: {root}", file=sys.stderr)
        return 1

    sessions = []
    for fpath in iter_session_files(root):
        parsed = detect_and_parse(fpath, agent=args.agent)
        if parsed and (parsed.turn_count > 0 or parsed.observations):
            sessions.append(parsed)

    summary = summarize_corpus(sessions)
    if args.json:
        out = asdict(summary)
        out["total_turns"] = summary.total_turns
        out["total_input"] = summary.total_input
        out["total_raw_tokens"] = summary.total_raw_tokens
        out["total_cost_weighted"] = round(summary.total_cost_weighted, 1)
        out["cache_hit_rate"] = round(summary.cache_hit_rate, 4)
        out["amplification_factor"] = round(summary.amplification_factor, 2)
        del out["read_sizes"]  # keep JSON summary compact
        print(json.dumps(out, indent=2))
    else:
        agent_for_weights = sessions[0].agent if sessions else "claude"
        print(format_markdown_report(summary, agent=agent_for_weights), end="")
    return 0


def cmd_audit(args: argparse.Namespace) -> int:
    root = Path(args.path).resolve()
    items = audit_directory(root, include_user=args.include_user)
    if args.json:
        print(json.dumps([asdict(i) for i in items], indent=2))
    else:
        print(format_audit_markdown(items), end="")
    return 0


def cmd_sanitize(args: argparse.Namespace) -> int:
    root = Path(args.path) if args.path else _default_claude_root()
    out_path = Path(args.out)
    sessions = []
    for fpath in iter_session_files(root):
        parsed = detect_and_parse(fpath, agent=args.agent)
        if parsed and (parsed.turn_count > 0 or parsed.observations):
            sessions.append(parsed)

    count = write_sanitized_json(sessions, out_path)
    print(f"Wrote {count} sanitized session records to {out_path}")
    return 0


def cmd_selftest(_args: argparse.Namespace) -> int:
    fixtures_dir = Path(__file__).resolve().parent.parent / "fixtures"
    claude_f = fixtures_dir / "claude_session.jsonl"
    codex_f = fixtures_dir / "codex_rollout.jsonl"
    gemini_f = fixtures_dir / "gemini_chat.json"

    # 1. Verify Claude Code multi-line streaming deduplication & cache split
    c_sess = detect_and_parse(claude_f, agent="auto")
    assert c_sess is not None and c_sess.agent == "claude", "Failed to detect Claude fixture"
    assert c_sess.turn_count == 3, f"Expected 3 deduplicated turns, got {c_sess.turn_count}"
    assert c_sess.duplicate_api_lines_skipped == 1, "Expected 1 split streaming line merged"
    # Turn 0 had placeholder output_tokens=3 on line 1 and final output_tokens=120 on line 2
    assert c_sess.turns[0].output == 120, f"Expected final output_tokens=120, got {c_sess.turns[0].output}"
    assert c_sess.cache_write == 2800 and c_sess.cache_write_5m == 2500 and c_sess.cache_write_1h == 300
    assert c_sess.cache_read == 49500 and c_sess.uncached_input == 20
    assert len(c_sess.observations) == 2
    assert c_sess.observations[1].followed_search is True, "Expected Read after Grep to flag followed_search"

    # 2. Verify Codex CLI subset caching & duplicate event skipping
    x_sess = detect_and_parse(codex_f, agent="auto")
    assert x_sess is not None and x_sess.agent == "codex", "Failed to detect Codex fixture"
    assert x_sess.turn_count == 2, f"Expected 2 turns (1 duplicate skipped), got {x_sess.turn_count}"
    assert x_sess.duplicate_api_lines_skipped == 1
    # Turn 0: input=10000, cached=8000 -> uncached=2000, cache_read=8000
    # Turn 1: input=12000, cached=11000 -> uncached=1000, cache_read=11000
    assert x_sess.uncached_input == 3000 and x_sess.cache_read == 19000
    assert x_sess.output == 450 and x_sess.reasoning == 180
    assert len(x_sess.observations) == 2
    assert x_sess.observations[1].is_ranged_read is True

    # 3. Verify Gemini CLI subset caching & thoughts inclusion
    g_sess = detect_and_parse(gemini_f, agent="auto")
    assert g_sess is not None and g_sess.agent == "gemini", "Failed to detect Gemini fixture"
    assert g_sess.turn_count == 2
    # Turn 0: prompt=12000, cached=9000 -> uncached=3000; cand=150 + thoughts=60 -> output=210
    # Turn 1: prompt=14000, cached=12000 -> uncached=2000; cand=100 + thoughts=40 -> output=140
    assert g_sess.uncached_input == 5000 and g_sess.cache_read == 21000
    assert g_sess.output == 350 and g_sess.reasoning == 100
    assert len(g_sess.observations) == 2

    print("tokscope selftest: ALL 3 AGENT FIXTURES PASSED (Claude Code, Codex CLI, Gemini CLI)")
    return 0


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="tokscope",
        description="Session token attribution, history amplification, and static-prefix auditing.",
    )
    sub = p.add_subparsers(dest="command", required=True)

    p_an = sub.add_parser("analyze", help="Analyze session transcripts and attribute tokens/cost")
    p_an.add_argument("path", nargs="?", default=None, help="Transcript file or directory (default: ~/.claude/projects)")
    p_an.add_argument("--agent", choices=["auto", "claude", "codex", "gemini"], default="auto")
    p_an.add_argument("--json", action="store_true", help="Output JSON summary instead of Markdown")
    p_an.set_defaults(func=cmd_analyze)

    p_au = sub.add_parser("audit", help="Audit static prefix tokens (AGENTS.md, CLAUDE.md, skills, MCP)")
    p_au.add_argument("path", nargs="?", default=".", help="Project workspace directory (default: .)")
    p_au.add_argument("--include-user", action="store_true", help="Include user-level global configs (~/.claude, ~/.gemini)")
    p_au.add_argument("--json", action="store_true", help="Output JSON instead of Markdown")
    p_au.set_defaults(func=cmd_audit)

    p_sa = sub.add_parser("sanitize", help="Export privacy-safe per-session aggregate metrics")
    p_sa.add_argument("path", nargs="?", default=None, help="Transcript file or directory (default: ~/.claude/projects)")
    p_sa.add_argument("--agent", choices=["auto", "claude", "codex", "gemini"], default="auto")
    p_sa.add_argument("--out", required=True, help="Output JSON file path (e.g. data/public/sessions.json)")
    p_sa.set_defaults(func=cmd_sanitize)

    p_st = sub.add_parser("selftest", help="Run schema & accounting self-tests on synthetic fixtures")
    p_st.set_defaults(func=cmd_selftest)

    return p


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
