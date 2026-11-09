#!/usr/bin/env python3
"""eai: Unified zero-dependency CLI for efficient-ai (`analyze`, `audit`, `outline`, `squeeze`, `setup`).

Works immediately with standard Python 3.10+ (zero pip packages or Rust build required),
and automatically accelerates `outline` and `squeeze` with the compiled Rust binaries when present.
"""

from __future__ import annotations

import ast
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT / "tools" / "tokscope"))
sys.path.insert(0, str(REPO_ROOT / "hooks"))

from tokscope.cli import main as tokscope_main  # noqa: E402
from claude_pre_tool import rewrite_shell_cmd  # noqa: E402


def _rust_bin(name: str) -> Path | None:
    exe = f"{name}.exe" if sys.platform == "win32" else name
    candidate = REPO_ROOT / "tools" / name / "target" / "release" / exe
    if candidate.is_file():
        return candidate
    which_hit = shutil.which(name)
    return Path(which_hit) if which_hit else None


def _py_outline_file(path: Path, symbol: str | None = None, line_range: str | None = None) -> int:
    if not path.exists():
        print(f"Error: path not found: {path}", file=sys.stderr)
        return 1
    src = path.read_text(encoding="utf-8", errors="replace")
    lines = src.splitlines()
    total = len(lines)

    if line_range:
        m = re.match(r"L?(\d+)[:-]L?(\d+)", line_range.strip(), re.I)
        if not m:
            print(f"Error: invalid range '{line_range}' (expected START-END)", file=sys.stderr)
            return 2
        s, e = max(1, int(m.group(1))), min(total, int(m.group(2)))
        print(f"# {path} [L{s}-L{e} of {total}]")
        for idx in range(s - 1, e):
            print(f"{idx + 1:>4}: {lines[idx]}")
        return 0

    spans: list[tuple[str, int, int, int, str]] = []  # (name, start, end, indent, sig)
    if path.suffix.lower() in (".py", ".pyi"):
        try:
            tree = ast.parse(src)
            for node in ast.walk(tree):
                if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                    s_line = node.lineno
                    e_line = getattr(node, "end_lineno", s_line) or s_line
                    raw_l = lines[s_line - 1]
                    ind = len(raw_l) - len(raw_l.lstrip())
                    sig = raw_l.strip()
                    spans.append((node.name, s_line, e_line, ind, sig))
            spans.sort(key=lambda x: x[1])
        except SyntaxError:
            pass

    if not spans:
        # Generic regex fallback for Rust/TS/JS/Go/Markdown
        decl_re = re.compile(
            r"^(\s*)(?:pub(?:\([^)]*\))?\s+|export\s+(?:default\s+)?|async\s+)*"
            r"(def|class|fn|struct|enum|trait|impl|function|interface|func|#{1,4})\s+([A-Za-z0-9_:<>\s]+)"
        )
        for idx, l in enumerate(lines):
            m = decl_re.match(l)
            if m:
                ind = len(m.group(1))
                name = m.group(3).strip().split("(")[0].split("{")[0].strip()
                spans.append((name, idx + 1, min(total, idx + 25), ind, l.strip().rstrip("{").strip()))

    if symbol:
        q = symbol.lower().split(".")[-1]
        matched = [sp for sp in spans if sp[0].lower() == q] or [sp for sp in spans if q in sp[0].lower()]
        if not matched:
            print(f"No symbol matching '{symbol}' in {path}; showing outline:", file=sys.stderr)
        else:
            for name, s, e, _, _ in matched:
                print(f"# {path} :: {name} [L{s}-L{e}]")
                for idx in range(s - 1, min(total, e)):
                    print(f"{idx + 1:>4}: {lines[idx]}")
            return 0

    print(f"# {path} ({total} lines)")
    for _, s, e, ind, sig in spans:
        print(f"{' ' * ind}[L{s}-L{e}] {sig}")
    print(f"[{len(spans)} symbols across {total} lines; use '--symbol <name>' or '--lines <start>-<end>']")
    return 0


def cmd_outline(argv: list[str]) -> int:
    rust_exe = _rust_bin("outline")
    if rust_exe is not None:
        return subprocess.call([str(rust_exe), *argv])

    if not argv or argv[0] in ("-h", "--help"):
        print("Usage: eai outline <file> [--symbol <name>] [--lines <start>-<end>]")
        return 0

    if argv[0] == "search":
        pat = argv[1] if len(argv) > 1 else ""
        root = Path(argv[2]) if len(argv) > 2 else Path(".")
        for p in sorted(root.rglob("*")):
            if p.is_file() and p.suffix in (".py", ".rs", ".ts", ".js", ".go", ".md") and ".venv" not in p.parts and ".git" not in p.parts:
                txt = p.read_text(encoding="utf-8", errors="ignore")
                if pat in txt:
                    print(f"# {p}")
                    for i, line in enumerate(txt.splitlines(), 1):
                        if pat in line:
                            print(f"  {i:>4}: {line.strip()}")
        return 0

    target = Path(argv[0])
    sym = argv[argv.index("--symbol") + 1] if "--symbol" in argv else None
    rng = argv[argv.index("--lines") + 1] if "--lines" in argv else None
    return _py_outline_file(target, symbol=sym, line_range=rng)


def cmd_squeeze(argv: list[str]) -> int:
    rust_exe = _rust_bin("squeeze")
    if rust_exe is not None:
        return subprocess.call([str(rust_exe), *argv])

    if len(argv) >= 2 and argv[0] == "--rewrite":
        print(rewrite_shell_cmd(argv[1]))
        return 0

    if "--" in argv:
        cmd = argv[argv.index("--") + 1 :]
    else:
        cmd = argv

    if cmd:
        proc = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace")
        raw = (proc.stdout or "") + ("\n" + proc.stderr if proc.stderr else "")
        code = proc.returncode
    else:
        raw = sys.stdin.read()
        code = 0

    ansi_re = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")
    cleaned = [ansi_re.sub("", l.rsplit("\r", 1)[-1]) for l in raw.splitlines()]
    kept: list[str] = []
    elided = 0
    for l in cleaned:
        t = l.strip()
        if ("::" in t and t.endswith("PASSED")) or (t.startswith("test ") and t.endswith(" ... ok")):
            elided += 1
            continue
        kept.append(l)

    print("\n".join(kept))
    if elided > 0:
        log_dir = Path(".eai") / "logs"
        log_dir.mkdir(parents=True, exist_ok=True)
        last_log = log_dir / "last.log"
        last_log.write_text(raw, encoding="utf-8")
        print(f"[{elided} passing lines elided ({len(cleaned)} -> {len(kept)} lines); full log: {last_log}]")
    return code


def cmd_setup(argv: list[str]) -> int:
    target = Path(argv[argv.index("--project") + 1]) if "--project" in argv else Path(".")
    target = target.resolve()

    agents_dst = target / "AGENTS.md"
    claude_dst = target / "CLAUDE.md"
    if not agents_dst.exists():
        shutil.copyfile(REPO_ROOT / "prompts" / "AGENTS.md", agents_dst)
        print(f"Created {agents_dst} (227 tokens)")
    else:
        print(f"Skipped {agents_dst} (already exists)")

    if not claude_dst.exists():
        shutil.copyfile(REPO_ROOT / "prompts" / "CLAUDE.md", claude_dst)
        print(f"Created {claude_dst} (@AGENTS.md shim)")

    print("Setup complete. Run `python scripts/eai.py audit .` to inspect your static prefix.")
    return 0


def main(argv: list[str] | None = None) -> int:
    args = list(sys.argv[1:] if argv is None else argv)
    if not args or args[0] in ("-h", "--help"):
        print(
            "eai — Unified CLI for efficient-ai\n\n"
            "Commands:\n"
            "  eai analyze [path]                 Profile session tokens, cache hits, & history amplification\n"
            "  eai audit [path] [--include-user]  Audit static prefix (AGENTS.md, CLAUDE.md, skills, MCP)\n"
            "  eai outline <file> [--symbol S]    Structure-aware file outline & targeted symbol reader\n"
            "  eai squeeze -- <cmd> [args...]     Lossless output compression (spills raw to .eai/logs/last.log)\n"
            "  eai setup [--project <dir>]        Install lean AGENTS.md + CLAUDE.md shim into a project"
        )
        return 0

    sub = args[0]
    rest = args[1:]
    if sub in ("analyze", "audit", "sanitize", "selftest"):
        return tokscope_main([sub, *rest])
    if sub == "outline":
        return cmd_outline(rest)
    if sub == "squeeze":
        return cmd_squeeze(rest)
    if sub == "setup":
        return cmd_setup(rest)

    print(f"Unknown command '{sub}'. Run with --help for usage.", file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
