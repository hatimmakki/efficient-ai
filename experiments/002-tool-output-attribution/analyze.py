"""Experiment 002 - where do tool-output tokens go in real Claude Code sessions?

Scans local Claude Code transcripts (~/.claude/projects/**/*.jsonl) and
attributes tool-output tokens to tool categories.

PRIVACY: prints aggregates only. No file contents, paths, prompts or command
lines are printed. Shell commands are reduced to their program name, and only
if it is on a public allowlist; anything else is reported as "other".
Never commit raw transcripts.

Token counts use tiktoken o200k_base as a proxy (Claude's tokenizer differs).
Each tool output is counted once; re-sending through history is NOT counted.

Usage:  python analyze.py [TRANSCRIPTS_ROOT]
"""

from __future__ import annotations

import json
import re
import statistics
import sys
from collections import Counter, defaultdict
from pathlib import Path

import tiktoken

ENC = tiktoken.get_encoding("o200k_base")

SEARCH_PROGRAMS = {"rg", "grep", "egrep", "find", "fd", "ag", "ls", "tree",
                   "select-string", "get-childitem", "git grep"}
# Public program names we are willing to report individually.
KNOWN_PROGRAMS = {
    "git", "gh", "npm", "npx", "pnpm", "yarn", "node", "bun", "deno",
    "python", "python3", "py", "pip", "uv", "pytest", "poetry",
    "cargo", "rustc", "go", "make", "cmake", "dotnet", "java", "mvn", "gradle",
    "docker", "kubectl", "curl", "wget", "cat", "head", "tail", "sed", "awk",
    "jq", "echo", "cd", "mkdir", "rm", "cp", "mv", "wc", "sort", "tsc", "eslint",
    "prettier", "ruff", "mypy", "flutter", "dart", "adb", "powershell", "pwsh",
    "get-content", "test-path", "set-location", "write-output",
}
PROG_RE = re.compile(r"^\s*(?:sudo\s+)?(?:[A-Z_]+=\S+\s+)*(git\s+grep|[\w.\-]+)", re.I)
# Leading "cd <dir> &&" / "cd <dir>;" / "Set-Location <dir>;" prefixes hide the real program.
CD_PREFIX_RE = re.compile(r"^\s*(?:cd|set-location|pushd)\s+(?:\"[^\"]*\"|'[^']*'|\S+)\s*(?:&&|;)\s*", re.I)


def count(text: str) -> int:
    return len(ENC.encode(text, disallowed_special=()))


def program_of(cmd: str) -> str:
    cmd = cmd or ""
    while (m := CD_PREFIX_RE.match(cmd)):
        cmd = cmd[m.end():]
    m = PROG_RE.match(cmd)
    if not m:
        return "other"
    prog = m.group(1).lower()
    prog = re.sub(r"\.exe$", "", prog)
    return prog if (prog in KNOWN_PROGRAMS or prog in SEARCH_PROGRAMS) else "other"


def result_text(content) -> str:
    if isinstance(content, list):
        return "\n".join(x.get("text", "") for x in content if isinstance(x, dict))
    return str(content or "")


def main() -> None:
    root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path.home() / ".claude" / "projects"
    tok_by_cat: Counter[str] = Counter()
    calls_by_cat: Counter[str] = Counter()
    tok_by_prog: Counter[str] = Counter()
    calls_by_prog: Counter[str] = Counter()
    read_sizes: list[int] = []
    read_full = read_ranged = 0
    read_after_search = 0
    sessions = 0

    for f in root.rglob("*.jsonl"):
        sessions += 1
        meta: dict[str, tuple[str, str, bool]] = {}  # id -> (category, program, ranged)
        last_cat = None
        for line in f.open(encoding="utf-8", errors="ignore"):
            try:
                o = json.loads(line)
            except json.JSONDecodeError:
                continue
            content = (o.get("message") or {}).get("content")
            if not isinstance(content, list):
                continue
            for b in content:
                if not isinstance(b, dict):
                    continue
                if b.get("type") == "tool_use":
                    name, inp = b.get("name", "?"), b.get("input") or {}
                    prog, ranged = "", False
                    if name in ("Bash", "PowerShell"):
                        prog = program_of(inp.get("command", ""))
                        cat = "Search (shell)" if prog in SEARCH_PROGRAMS else f"{name} (other)"
                    elif name in ("Grep", "Glob"):
                        cat = "Search (Grep/Glob)"
                    elif name.startswith("mcp__"):
                        cat = "MCP"
                    else:
                        cat = name
                    if name == "Read":
                        ranged = bool(inp.get("offset") or inp.get("limit"))
                        if last_cat and last_cat.startswith("Search"):
                            read_after_search += 1
                    meta[b.get("id")] = (cat, prog, ranged)
                    calls_by_cat[cat] += 1
                    if prog:
                        calls_by_prog[prog] += 1
                    last_cat = cat
                elif b.get("type") == "tool_result":
                    cat, prog, ranged = meta.get(b.get("tool_use_id"), ("unknown", "", False))
                    n = count(result_text(b.get("content")))
                    tok_by_cat[cat] += n
                    if prog:
                        tok_by_prog[prog] += n
                    if cat == "Read":
                        read_sizes.append(n)
                        if ranged:
                            read_ranged += 1
                        else:
                            read_full += 1

    total = sum(tok_by_cat.values()) or 1
    print(f"sessions: {sessions}; tool-output tokens (o200k est.): {total:,}\n")
    print("| category | calls | tokens | share | tokens/call |\n|---|---:|---:|---:|---:|")
    for cat, t in tok_by_cat.most_common(12):
        n = calls_by_cat[cat] or 1
        print(f"| {cat} | {calls_by_cat[cat]:,} | {t:,} | {100*t/total:.1f}% | {t//n:,} |")

    shell_total = sum(tok_by_prog.values()) or 1
    print("\n**Shell output by program** (top 12)\n")
    print("| program | calls | tokens | share of shell | tokens/call |\n|---|---:|---:|---:|---:|")
    for prog, t in tok_by_prog.most_common(12):
        n = calls_by_prog[prog] or 1
        print(f"| {prog} | {calls_by_prog[prog]:,} | {t:,} | {100*t/shell_total:.1f}% | {t//n:,} |")

    if read_sizes:
        s = sorted(read_sizes)
        p = lambda q: s[min(len(s) - 1, int(q * len(s)))]
        top10 = sum(s[int(0.9 * len(s)):]) / (sum(s) or 1)
        print("\n**Read tool**\n")
        print(f"- calls: {len(s):,} (whole-file: {read_full:,}, ranged: {read_ranged:,})")
        print(f"- tokens/read: median {statistics.median(s):,.0f}, p90 {p(0.9):,}, p99 {p(0.99):,}, max {s[-1]:,}")
        print(f"- largest 10% of reads = {100*top10:.1f}% of read tokens")
        print(f"- reads immediately after a search call: {read_after_search:,}")


if __name__ == "__main__":
    main()
