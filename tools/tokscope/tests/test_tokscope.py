"""Unit & integration tests for tokscope, hook adapters, and plugin marketplace manifests."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import unittest
from pathlib import Path

TESTS_DIR = Path(__file__).resolve().parent
TOKSCOPE_ROOT = TESTS_DIR.parent
REPO_ROOT = TOKSCOPE_ROOT.parent.parent
FIXTURES = TOKSCOPE_ROOT / "fixtures"

sys.path.insert(0, str(TOKSCOPE_ROOT))
sys.path.insert(0, str(REPO_ROOT / "hooks"))

from tokscope.attribution import format_markdown_report, summarize_corpus  # noqa: E402
from tokscope.audit import audit_directory  # noqa: E402
from tokscope.cli import main as cli_main  # noqa: E402
from tokscope.parsers import detect_and_parse  # noqa: E402
from tokscope.sanitize import sanitize_sessions  # noqa: E402


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
        grep_first = summary.first_tok_by_cat["Search (Grep/Glob)"]
        grep_resent = summary.resent_tok_by_cat["Search (Grep/Glob)"]
        self.assertEqual(grep_resent, grep_first * 3)

        read_first = summary.first_tok_by_cat["Read (tool)"]
        read_resent = summary.resent_tok_by_cat["Read (tool)"]
        self.assertEqual(read_resent, read_first * 2)

    def test_mixed_agent_cost_weights_per_session(self) -> None:
        c = detect_and_parse(FIXTURES / "claude_session.jsonl")
        a = detect_and_parse(FIXTURES / "antigravity_transcript.jsonl")
        x = detect_and_parse(FIXTURES / "codex_rollout.jsonl")
        g = detect_and_parse(FIXTURES / "gemini_chat.json")
        assert c and a and x and g
        summary = summarize_corpus([c, a, x, g])
        expected_total = (
            c.cost_weighted_tokens()
            + a.cost_weighted_tokens()
            + x.cost_weighted_tokens()
            + g.cost_weighted_tokens()
        )
        accumulated = (
            summary.uncached_cost_weighted
            + summary.cache_write_cost_weighted
            + summary.cache_read_cost_weighted
            + summary.output_cost_weighted
        )
        self.assertAlmostEqual(accumulated, expected_total, places=2)
        report = format_markdown_report(summary)
        self.assertIn("mixed corpus", report)

    def test_sanitize_strips_sensitive_fields(self) -> None:
        sess = detect_and_parse(FIXTURES / "claude_session.jsonl")
        assert sess is not None
        records = sanitize_sessions([sess])
        self.assertEqual(len(records), 1)
        rec = records[0]
        self.assertEqual(len(rec["session_id"]), 12)
        self.assertNotIn("src/parser.py", str(rec))

    def test_audit_directory(self) -> None:
        items = audit_directory(REPO_ROOT, include_user=False)
        self.assertIsInstance(items, list)


class TestHooksAndManifests(unittest.TestCase):
    def test_all_plugin_manifests_are_valid_json(self) -> None:
        manifests = [
            REPO_ROOT / ".claude-plugin" / "plugin.json",
            REPO_ROOT / ".claude-plugin" / "marketplace.json",
            REPO_ROOT / ".antigravity-plugin" / "plugin.json",
            REPO_ROOT / "hooks.json",
            REPO_ROOT / "hooks" / "hooks.json",
            REPO_ROOT / "hooks" / "claude.settings.json",
            REPO_ROOT / "hooks" / "gemini.settings.json",
            REPO_ROOT / ".codex-plugin" / "plugin.json",
            REPO_ROOT / ".agents" / "plugins" / "marketplace.json",
            REPO_ROOT / "gemini-extension.json",
        ]
        for m in manifests:
            self.assertTrue(m.is_file(), f"Missing manifest: {m}")
            data = json.loads(m.read_text(encoding="utf-8"))
            self.assertIsInstance(data, dict)

        # Verify Codex marketplace uses a valid remote URL, not "./"
        codex_mp = json.loads((REPO_ROOT / ".agents" / "plugins" / "marketplace.json").read_text(encoding="utf-8"))
        self.assertTrue(codex_mp["plugins"][0]["source"]["url"].startswith("https://"))

        # Verify single-source policy stays synced between prompts/AGENTS.md and rules/AGENTS.md
        p_md = (REPO_ROOT / "prompts" / "AGENTS.md").read_text(encoding="utf-8").strip()
        r_md = (REPO_ROOT / "rules" / "AGENTS.md").read_text(encoding="utf-8").strip()
        self.assertEqual(p_md, r_md)

        # Verify all 5 plugin manifests + npm launcher stay version-synced
        versions = {
            json.loads((REPO_ROOT / ".claude-plugin" / "plugin.json").read_text(encoding="utf-8"))["version"],
            json.loads((REPO_ROOT / ".claude-plugin" / "marketplace.json").read_text(encoding="utf-8"))["plugins"][0]["version"],
            json.loads((REPO_ROOT / ".antigravity-plugin" / "plugin.json").read_text(encoding="utf-8"))["version"],
            json.loads((REPO_ROOT / ".codex-plugin" / "plugin.json").read_text(encoding="utf-8"))["version"],
            json.loads((REPO_ROOT / "gemini-extension.json").read_text(encoding="utf-8"))["version"],
            json.loads((REPO_ROOT / "packages" / "cli" / "package.json").read_text(encoding="utf-8"))["version"],
        }
        self.assertEqual(len(versions), 1, f"Plugin manifests have mismatched versions: {versions}")

    def test_hook_adapters_python_fallback(self) -> None:
        env = dict(os.environ, EAI_FORCE_PY_HOOK="1")
        hook_script = str(REPO_ROOT / "hooks" / "eai_hook.py")

        # Claude PreToolUse Bash rewrite
        res_c = subprocess.run(
            [sys.executable, hook_script, "claude"],
            input=json.dumps({"tool_name": "Bash", "tool_input": {"command": "git status"}}),
            text=True,
            capture_output=True,
            env=env,
            check=True,
        )
        out_c = json.loads(res_c.stdout)
        self.assertEqual(out_c["hookSpecificOutput"]["updatedInput"]["command"], "git status -sb")

        # Antigravity PreToolUse overwrite
        res_a = subprocess.run(
            [sys.executable, hook_script, "antigravity"],
            input=json.dumps({
                "hookEventName": "PreToolUse",
                "toolCall": {"name": "run_command", "args": {"CommandLine": "git log"}},
            }),
            text=True,
            capture_output=True,
            env=env,
            check=True,
        )
        out_a = json.loads(res_a.stdout)
        self.assertEqual(out_a["decision"], "allow")
        self.assertEqual(out_a["overwrite"]["CommandLine"], "git log --oneline -n 15")

        # Gemini BeforeTool rewrite
        res_g = subprocess.run(
            [sys.executable, hook_script, "gemini"],
            input=json.dumps({"tool_name": "run_shell_command", "tool_input": {"command": "git status"}}),
            text=True,
            capture_output=True,
            env=env,
            check=True,
        )
        out_g = json.loads(res_g.stdout)
        self.assertEqual(out_g["decision"], "allow")
        self.assertEqual(out_g["tool_input"]["command"], "git status -sb")


if __name__ == "__main__":
    unittest.main()
