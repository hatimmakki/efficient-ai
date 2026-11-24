#!/usr/bin/env python3
"""Experiment 009: Turnkey Live Paired Agent A/B Harness (Control vs. efficient-ai Treatment).

Measures real end-to-end solve rate (quality non-inferiority), billed tokens, prompt-cache hit
rate, wall time, and USD spend across Claude Code, Antigravity, Gemini CLI, and Codex CLI.

Usage:
  # 1. Verify all tasks, failing starter repos, reference patches & graders (0 API cost):
  python experiments/009-live-paired-ab/run_paired_ab.py --dry-run

  # 2. Run real live paired A/B on Claude Code / Antigravity / Gemini CLI / Codex CLI:
  python experiments/009-live-paired-ab/run_paired_ab.py --agent claude
  python experiments/009-live-paired-ab/run_paired_ab.py --agent antigravity
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent


@dataclass
class TaskSpec:
    task_id: str
    title: str
    prompt: str
    files: dict[str, str]
    reference_patch: dict[str, str]
    test_cmd: list[str]


def _padded_py_module(target_fn: str, helper_count: int = 28) -> str:
    """Create a realistic >250-line Python module so outline-first reads have true leverage."""
    lines = ['"""Production analytics service module."""\n\nfrom __future__ import annotations\n']
    for i in range(1, helper_count + 1):
        lines.append(
            f"def metric_helper_{i}(values: list[float], scale: float = 1.0) -> float:\n"
            f"    \"\"\"Compute normalized metric bucket #{i}.\"\"\"\n"
            f"    if not values:\n"
            f"        return 0.0\n"
            f"    total = sum(v * scale for v in values)\n"
            f"    return round(total / len(values), 4)\n\n"
        )
    lines.append(target_fn)
    return "\n".join(lines)


def build_tasks() -> list[TaskSpec]:
    # Task 1: Fix Anthropic vs OpenAI cache accounting bug inside a 260-line module
    t1_bug = (
        "def compute_uncached_input(provider: str, input_tokens: int, cached_tokens: int) -> int:\n"
        "    \"\"\"Return uncached input tokens. Anthropic reports disjoint fields; OpenAI/Gemini report cached as a subset of input.\"\"\"\n"
        "    # BUG: subtracts cached_tokens even for Anthropic/Claude where fields are disjoint\n"
        "    return max(0, input_tokens - cached_tokens)\n"
    )
    t1_fix = (
        "def compute_uncached_input(provider: str, input_tokens: int, cached_tokens: int) -> int:\n"
        "    \"\"\"Return uncached input tokens. Anthropic reports disjoint fields; OpenAI/Gemini report cached as a subset of input.\"\"\"\n"
        "    if provider.lower() in ('claude', 'anthropic', 'antigravity'):\n"
        "        return max(0, input_tokens)\n"
        "    return max(0, input_tokens - cached_tokens)\n"
    )
    t1_test = (
        "import unittest\n"
        "from billing import compute_uncached_input\n\n"
        "class TestBilling(unittest.TestCase):\n"
        "    def test_claude_disjoint(self):\n"
        "        self.assertEqual(compute_uncached_input('claude', 500, 15000), 500)\n"
        "    def test_antigravity_disjoint(self):\n"
        "        self.assertEqual(compute_uncached_input('antigravity', 2000, 14000), 2000)\n"
        "    def test_openai_subset(self):\n"
        "        self.assertEqual(compute_uncached_input('openai', 10000, 8000), 2000)\n"
        "    def test_gemini_subset(self):\n"
        "        self.assertEqual(compute_uncached_input('gemini', 12000, 9000), 3000)\n\n"
        "if __name__ == '__main__':\n"
        "    unittest.main()\n"
    )

    # Task 2: Fix batch observation masking boundary snap (Exp 007 invariant)
    t2_bug = (
        "def batch_mask_boundary(total_obs: int, batch_k: int, keep_m: int) -> int:\n"
        "    \"\"\"Return number of leading observations eligible to mask at fixed K boundaries while keeping newest M verbatim.\"\"\"\n"
        "    # BUG: returns sliding window instead of snapping down to a multiple of batch_k\n"
        "    return max(0, total_obs - keep_m)\n"
    )
    t2_fix = (
        "def batch_mask_boundary(total_obs: int, batch_k: int, keep_m: int) -> int:\n"
        "    \"\"\"Return number of leading observations eligible to mask at fixed K boundaries while keeping newest M verbatim.\"\"\"\n"
        "    eligible = max(0, total_obs - keep_m)\n"
        "    k = max(1, batch_k)\n"
        "    return (eligible // k) * k\n"
    )
    t2_test = (
        "import unittest\n"
        "from masking import batch_mask_boundary\n\n"
        "class TestMasking(unittest.TestCase):\n"
        "    def test_below_threshold(self):\n"
        "        self.assertEqual(batch_mask_boundary(17, 12, 6), 0)\n"
        "    def test_exact_first_boundary(self):\n"
        "        self.assertEqual(batch_mask_boundary(18, 12, 6), 12)\n"
        "    def test_stays_frozen_between_boundaries(self):\n"
        "        for n in range(18, 30):\n"
        "            self.assertEqual(batch_mask_boundary(n, 12, 6), 12)\n"
        "    def test_second_boundary(self):\n"
        "        self.assertEqual(batch_mask_boundary(30, 12, 6), 24)\n\n"
        "if __name__ == '__main__':\n"
        "    unittest.main()\n"
    )

    # Task 3: Fix consecutive duplicate line collapser in a 260-line stream utility
    t3_bug = (
        "def collapse_duplicates(lines: list[str]) -> list[str]:\n"
        "    \"\"\"Collapse consecutive identical non-empty lines into `<line> [xN]`.\"\"\"\n"
        "    # BUG: drops non-consecutive duplicates using a set and omits [xN] counts\n"
        "    seen = set()\n"
        "    out = []\n"
        "    for line in lines:\n"
        "        if line not in seen:\n"
        "            seen.add(line)\n"
        "            out.append(line)\n"
        "    return out\n"
    )
    t3_fix = (
        "def collapse_duplicates(lines: list[str]) -> list[str]:\n"
        "    \"\"\"Collapse consecutive identical non-empty lines into `<line> [xN]`.\"\"\"\n"
        "    out: list[str] = []\n"
        "    i = 0\n"
        "    while i < len(lines):\n"
        "        line = lines[i]\n"
        "        run = 1\n"
        "        while i + run < len(lines) and lines[i + run] == line:\n"
        "            run += 1\n"
        "        if run > 1 and line.strip():\n"
        "            out.append(f'{line} [x{run}]')\n"
        "        else:\n"
        "            out.extend([line] * run)\n"
        "        i += run\n"
        "    return out\n"
    )
    t3_test = (
        "import unittest\n"
        "from stream_util import collapse_duplicates\n\n"
        "class TestStream(unittest.TestCase):\n"
        "    def test_consecutive_collapsed(self):\n"
        "        self.assertEqual(collapse_duplicates(['retry', 'retry', 'retry', 'ok']), ['retry [x3]', 'ok'])\n"
        "    def test_non_consecutive_preserved(self):\n"
        "        self.assertEqual(collapse_duplicates(['a', 'b', 'a']), ['a', 'b', 'a'])\n\n"
        "if __name__ == '__main__':\n"
        "    unittest.main()\n"
    )

    return [
        TaskSpec(
            task_id="T1-cache-accounting",
            title="Fix provider disjoint vs subset cache accounting",
            prompt="Fix `compute_uncached_input` in `billing.py` so `python -m unittest test_billing.py` passes. Do not modify tests.",
            files={"billing.py": _padded_py_module(t1_bug), "test_billing.py": t1_test},
            reference_patch={"billing.py": _padded_py_module(t1_fix)},
            test_cmd=[sys.executable, "-m", "unittest", "test_billing.py"],
        ),
        TaskSpec(
            task_id="T2-batch-boundary",
            title="Fix Exp 007 fixed-boundary batch masking snap",
            prompt="Fix `batch_mask_boundary` in `masking.py` so `python -m unittest test_masking.py` passes. Do not modify tests.",
            files={"masking.py": _padded_py_module(t2_bug), "test_masking.py": t2_test},
            reference_patch={"masking.py": _padded_py_module(t2_fix)},
            test_cmd=[sys.executable, "-m", "unittest", "test_masking.py"],
        ),
        TaskSpec(
            task_id="T3-duplicate-collapse",
            title="Fix consecutive duplicate line collapser",
            prompt="Fix `collapse_duplicates` in `stream_util.py` so `python -m unittest test_stream.py` passes. Do not modify tests.",
            files={"stream_util.py": _padded_py_module(t3_bug), "test_stream.py": t3_test},
            reference_patch={"stream_util.py": _padded_py_module(t3_fix)},
            test_cmd=[sys.executable, "-m", "unittest", "test_stream.py"],
        ),
    ]


def find_eai() -> Path:
    exe = "eai.exe" if sys.platform == "win32" else "eai"
    for c in (REPO_ROOT / "target" / "release" / exe, REPO_ROOT / "target" / "debug" / exe):
        if c.is_file():
            return c
    hit = shutil.which("eai")
    if hit:
        return Path(hit)
    raise FileNotFoundError("eai binary not found; build with `cargo build --release -p eai`.")


def materialize_workspace(root: Path, files: dict[str, str]) -> None:
    for rel, content in files.items():
        p = root / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(content, encoding="utf-8")


def grade_workspace(root: Path, test_cmd: list[str]) -> bool:
    proc = subprocess.run(test_cmd, cwd=root, capture_output=True, text=True)
    return proc.returncode == 0


def run_dry_validation(tasks: list[TaskSpec], eai: Path) -> int:
    print(f"Validating {len(tasks)} paired A/B benchmark tasks (dry-run, 0 API spend)...")
    for t in tasks:
        with tempfile.TemporaryDirectory() as td:
            ws = Path(td)
            materialize_workspace(ws, t.files)
            assert not grade_workspace(ws, t.test_cmd), f"{t.task_id}: starter unexpectedly passed!"

            # Initialize eai treatment workspace & verify outline finds target symbol cleanly
            subprocess.check_call(
                [str(eai), "init", "--project", str(ws), "--agents", "all", "--no-baseline"],
                stdout=subprocess.DEVNULL,
            )
            src_file = next(k for k in t.files if not k.startswith("test_"))
            out = subprocess.check_output([str(eai), "outline", str(ws / src_file)], text=True)
            assert "symbols across" in out, f"{t.task_id}: outline failed"

            # Apply reference patch and verify grader passes
            materialize_workspace(ws, t.reference_patch)
            assert grade_workspace(ws, t.test_cmd), f"{t.task_id}: reference solution failed!"
            print(f"  [PASS] {t.task_id}: starter=FAIL, outline=OK, reference=PASS")

    print(f"\nDry-run complete: ALL {len(tasks)}/{len(tasks)} tasks verified and ready for live paired runs.")
    return 0


def invoke_agent(agent: str, prompt: str, cwd: Path, timeout_s: int = 180) -> float:
    cmd_map = {
        "claude": ["claude", "-p", prompt, "--dangerously-skip-permissions"],
        "antigravity": ["agy", "-p", prompt],
        "gemini": ["gemini", "-p", prompt],
        "codex": ["codex", "exec", prompt],
    }
    cmd = cmd_map[agent]
    t0 = time.perf_counter()
    subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout_s)
    return time.perf_counter() - t0


def run_live_paired(agent: str, tasks: list[TaskSpec], eai: Path, out_md: Path) -> int:
    rows = []
    for t in tasks:
        res_pair = {}
        for arm in ("control", "treatment"):
            with tempfile.TemporaryDirectory(prefix=f"eai-ab-{t.task_id}-{arm}-") as td:
                ws = Path(td)
                materialize_workspace(ws, t.files)
                if arm == "treatment":
                    subprocess.check_call(
                        [str(eai), "init", "--project", str(ws), "--agents", agent, "--no-baseline"],
                        stdout=subprocess.DEVNULL,
                    )
                else:
                    subprocess.check_call(
                        [str(eai), "pause", "--project", str(ws)],
                        stdout=subprocess.DEVNULL,
                    )

                elapsed = invoke_agent(agent, t.prompt, ws)
                passed = grade_workspace(ws, t.test_cmd)
                an_raw = subprocess.check_output(
                    [str(eai), "analyze", "--project", str(ws), "--agent", agent, "--json"],
                    text=True,
                )
                an = json.loads(an_raw)
                res_pair[arm] = {
                    "passed": passed,
                    "wall_s": round(elapsed, 1),
                    "turns": an.get("parent_turns", 0) + an.get("subagent_turns", 0),
                    "usd": round(an.get("total_estimated_usd", 0.0), 4),
                    "cache_read": an.get("cache_read", 0),
                }
        rows.append((t, res_pair))

    lines = [
        f"# Experiment 009: Live Paired A/B Results (`{agent}`)\n",
        "| Task | Control Pass | Treatment Pass | Control USD | Treatment USD | Δ USD | Control Time | Treatment Time |",
        "|---|:---:|:---:|---:|---:|---:|---:|---:|",
    ]
    for t, rp in rows:
        c, tr = rp["control"], rp["treatment"]
        d_usd = 100.0 * (tr["usd"] - c["usd"]) / max(1e-6, c["usd"])
        lines.append(
            f"| `{t.task_id}` | {'PASS' if c['passed'] else 'FAIL'} | {'PASS' if tr['passed'] else 'FAIL'} "
            f"| ${c['usd']:.4f} | ${tr['usd']:.4f} | **{d_usd:+.1f}%** | {c['wall_s']}s | {tr['wall_s']}s |"
        )
    out_md.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print("\n".join(lines))
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true", help="Verify starter failures, outline, & reference passes (0 API cost)")
    ap.add_argument("--agent", choices=["claude", "antigravity", "gemini", "codex"], default="claude")
    ap.add_argument("--out", default=str(Path(__file__).resolve().parent / "RESULTS.md"))
    args = ap.parse_args()

    tasks = build_tasks()
    eai = find_eai()
    if args.dry_run:
        return run_dry_validation(tasks, eai)
    return run_live_paired(args.agent, tasks, eai, Path(args.out))


if __name__ == "__main__":
    raise SystemExit(main())
