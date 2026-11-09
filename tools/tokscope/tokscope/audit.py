"""Static-prefix auditor (`tokscope audit`) for instruction files, skills, and MCP configs."""

from __future__ import annotations

import json
import re
from dataclasses import dataclass
from pathlib import Path

try:
    import tiktoken
    ENC_O200K = tiktoken.get_encoding("o200k_base")
    ENC_CL100K = tiktoken.get_encoding("cl100k_base")
except ImportError:
    ENC_O200K = None
    ENC_CL100K = None

FRONTMATTER_RE = re.compile(r"^---\s*\r?\n(.*?)\r?\n---\s*(?:\r?\n|$)", re.DOTALL)


@dataclass
class PrefixItem:
    category: str
    label: str
    loaded_when: str  # "always (every turn)" or "on-demand"
    o200k_tokens: int
    cl100k_tokens: int
    note: str = ""


def _tok_pair(text: str) -> tuple[int, int]:
    if not text:
        return 0, 0
    if ENC_O200K is not None and ENC_CL100K is not None:
        return (
            len(ENC_O200K.encode(text, disallowed_special=())),
            len(ENC_CL100K.encode(text, disallowed_special=())),
        )
    est = max(1, int(len(text) / 3.8))
    return est, est


def audit_directory(root: Path, include_user: bool = False) -> list[PrefixItem]:
    """Scan workspace (and optionally user home config) for static prefix contributors."""
    items: list[PrefixItem] = []

    # 1. Instruction files in workspace
    for fname in ("AGENTS.md", "CLAUDE.md", "GEMINI.md", ".cursorrules", ".github/copilot-instructions.md"):
        fpath = root / fname
        if fpath.is_file():
            txt = fpath.read_text(encoding="utf-8", errors="ignore")
            o2, cl = _tok_pair(txt)
            note = "Paid on every turn"
            if "@AGENTS.md" in txt:
                note = "Imports @AGENTS.md"
            items.append(PrefixItem("Instruction file", fname, "always (every turn)", o2, cl, note))

    # 2. Project skills
    for skill_dir_rel in (".claude/skills", ".gemini/skills", ".agents/skills", "skills"):
        sdir = root / skill_dir_rel
        if not sdir.is_dir():
            continue
        for smd in sorted(sdir.rglob("SKILL.md")):
            txt = smd.read_text(encoding="utf-8", errors="ignore")
            m = FRONTMATTER_RE.match(txt)
            fm_text = m.group(1) if m else txt[:300]
            fm_o2, fm_cl = _tok_pair(fm_text)
            body_o2, body_cl = _tok_pair(txt)
            rel_label = f"{skill_dir_rel}/{smd.parent.name}/SKILL.md"
            items.append(
                PrefixItem(
                    "Skill (idle metadata)",
                    rel_label,
                    "always (every turn)",
                    fm_o2,
                    fm_cl,
                    f"Full body if triggered: {body_o2:,} tok",
                )
            )

    # 3. Project MCP / settings configs
    for cfg_rel in (".mcp.json", ".claude/settings.json", ".gemini/settings.json"):
        cpath = root / cfg_rel
        if cpath.is_file():
            txt = cpath.read_text(encoding="utf-8", errors="ignore")
            o2, cl = _tok_pair(txt)
            min_o2 = o2
            try:
                parsed = json.loads(txt)
                minified = json.dumps(parsed, separators=(",", ":"))
                min_o2, _ = _tok_pair(minified)
            except Exception:
                pass
            savings = o2 - min_o2
            note = f"Minifying saves {savings:,} tok" if savings > 10 else "Compact"
            items.append(PrefixItem("Config / MCP", cfg_rel, "config / startup", o2, cl, note))

    # 4. Optional user-level global configs (labels anonymized to ~/...)
    if include_user:
        home = Path.home()
        for rel in (".claude/CLAUDE.md", ".gemini/GEMINI.md", ".codex/AGENTS.md"):
            upath = home / rel
            if upath.is_file():
                txt = upath.read_text(encoding="utf-8", errors="ignore")
                o2, cl = _tok_pair(txt)
                items.append(PrefixItem("User global instruction", f"~/{rel}", "always (every turn)", o2, cl, "Global prefix"))

        for sroot_rel in (".claude/skills", ".gemini/skills", ".gemini/config/skills"):
            sdir = home / sroot_rel
            if sdir.is_dir():
                skill_files = list(sdir.rglob("SKILL.md"))
                if skill_files:
                    tot_fm_o2 = tot_fm_cl = tot_body_o2 = 0
                    for smd in skill_files:
                        txt = smd.read_text(encoding="utf-8", errors="ignore")
                        m = FRONTMATTER_RE.match(txt)
                        fm_text = m.group(1) if m else txt[:300]
                        fo2, fcl = _tok_pair(fm_text)
                        bo2, _ = _tok_pair(txt)
                        tot_fm_o2 += fo2
                        tot_fm_cl += fcl
                        tot_body_o2 += bo2
                    items.append(
                        PrefixItem(
                            "User global skills (idle)",
                            f"~/{sroot_rel} ({len(skill_files)} skills)",
                            "always (every turn)",
                            tot_fm_o2,
                            tot_fm_cl,
                            f"Avg {tot_fm_o2 // len(skill_files)} tok/skill idle; {tot_body_o2:,} tok total bodies",
                        )
                    )

    return items


def format_audit_markdown(items: list[PrefixItem]) -> str:
    if not items:
        return "No static-prefix files (`AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, skills, or `.mcp.json`) found in target directory.\n"

    always_o2 = sum(i.o200k_tokens for i in items if i.loaded_when.startswith("always"))
    lines = [
        "# tokscope Static-Prefix Audit",
        "",
        "| Category | Source | Loaded | `o200k_base` | `cl100k_base` | Notes |",
        "|---|---|---|---:|---:|---|",
    ]
    for it in items:
        lines.append(
            f"| {it.category} | `{it.label}` | {it.loaded_when} | {it.o200k_tokens:,} | {it.cl100k_tokens:,} | {it.note} |"
        )

    # 50-turn session tax calculation:
    # Turn 1 writes to cache (1.25x), Turns 2..50 read from cache (49 * 0.1x = 4.9x) -> 6.15x input-token equivalents per session,
    # and 50x raw context exposure!
    raw_50 = always_o2 * 50
    cost_50 = int(always_o2 * (1.25 + 49 * 0.10))
    lines.extend([
        "",
        "## Session Tax Summary",
        "",
        f"- **Idle static prefix (`o200k_base`):** **{always_o2:,} tokens / turn**",
        f"- **50-turn raw context footprint:** **{raw_50:,} tokens**",
        f"- **50-turn billed cost (with 5m prompt caching):** **{cost_50:,} input-token equivalents** (`1.25×` write + `49 × 0.10×` reads; **{always_o2 * 50:,}** if cache is busted each turn)",
    ])
    return "\n".join(lines) + "\n"
