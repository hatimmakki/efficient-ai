"""Unit tests for tokscope parsers, attribution, static-prefix audit, and sanitization."""

from __future__ import annotations

import unittest
from pathlib import Path

from tokscope.attribution import summarize_corpus
from tokscope.audit import audit_directory
from tokscope.cli import main as cli_main
from tokscope.parsers import detect_and_parse
from tokscope.sanitize import sanitize_sessions


FIXTURES = Path(__file__).resolve().parent.parent / "fixtures"


class TestTokscope(unittest.TestCase):
    def test_selftest_cli(self) -> None:
        rc = cli_main(["selftest"])
        self.assertEqual(rc, 0)

    def test_claude_history_amplification(self) -> None:
        sess = detect_and_parse(FIXTURES / "claude_session.jsonl")
        self.assertIsNotNone(sess)
        assert sess is not None
        summary = summarize_corpus([sess])
        self.assertEqual(summary.parent_sessions, 1)
        self.assertEqual(summary.total_turns, 3)
        # Turn 0 tool output (Grep) is re-sent across 3 remaining turns (3 - 0 = 3x)
        # Turn 1 tool output (Read) is re-sent across 2 remaining turns (3 - 1 = 2x)
        grep_first = summary.first_tok_by_cat["Search (Grep/Glob)"]
        grep_resent = summary.resent_tok_by_cat["Search (Grep/Glob)"]
        self.assertEqual(grep_resent, grep_first * 3)

        read_first = summary.first_tok_by_cat["Read (tool)"]
        read_resent = summary.resent_tok_by_cat["Read (tool)"]
        self.assertEqual(read_resent, read_first * 2)

    def test_sanitize_strips_sensitive_fields(self) -> None:
        sess = detect_and_parse(FIXTURES / "claude_session.jsonl")
        assert sess is not None
        records = sanitize_sessions([sess])
        self.assertEqual(len(records), 1)
        rec = records[0]
        self.assertEqual(len(rec["session_id"]), 12)
        self.assertNotIn("src/parser.py", str(rec))

    def test_audit_directory(self) -> None:
        repo_root = FIXTURES.parent.parent.parent
        items = audit_directory(repo_root, include_user=False)
        self.assertIsInstance(items, list)


if __name__ == "__main__":
    unittest.main()
