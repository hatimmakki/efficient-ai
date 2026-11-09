"""Corpus aggregation and history-amplification attribution across sessions."""

from __future__ import annotations

import statistics
from collections import Counter
from dataclasses import dataclass, field

from .models import DEFAULT_WEIGHTS, SessionMetrics


@dataclass
class CorpusSummary:
    """Aggregated token economics across multiple SessionMetrics."""

    total_files: int = 0
    parent_sessions: int = 0
    subagent_sessions: int = 0
    parent_turns: int = 0
    subagent_turns: int = 0
    duplicate_api_lines_skipped: int = 0

    uncached_input: int = 0
    cache_write: int = 0
    cache_write_5m: int = 0
    cache_write_1h: int = 0
    cache_read: int = 0
    output: int = 0
    reasoning: int = 0

    parent_cost_weighted: float = 0.0
    subagent_cost_weighted: float = 0.0

    # Tool attribution: first-occurrence vs cumulative re-sent across remaining turns
    calls_by_cat: Counter[str] = field(default_factory=Counter)
    first_tok_by_cat: Counter[str] = field(default_factory=Counter)
    resent_tok_by_cat: Counter[str] = field(default_factory=Counter)

    calls_by_prog: Counter[str] = field(default_factory=Counter)
    first_tok_by_prog: Counter[str] = field(default_factory=Counter)
    resent_tok_by_prog: Counter[str] = field(default_factory=Counter)

    read_sizes: list[int] = field(default_factory=list)
    read_full_count: int = 0
    read_ranged_count: int = 0
    read_after_search_count: int = 0

    @property
    def total_turns(self) -> int:
        return self.parent_turns + self.subagent_turns

    @property
    def total_input(self) -> int:
        return self.uncached_input + self.cache_write + self.cache_read

    @property
    def total_raw_tokens(self) -> int:
        return self.total_input + self.output

    @property
    def total_cost_weighted(self) -> float:
        return self.parent_cost_weighted + self.subagent_cost_weighted

    @property
    def cache_hit_rate(self) -> float:
        return (self.cache_read / self.total_input) if self.total_input > 0 else 0.0

    @property
    def amplification_factor(self) -> float:
        new_in = self.uncached_input + self.cache_write
        return (self.total_input / new_in) if new_in > 0 else 1.0


def summarize_corpus(sessions: list[SessionMetrics]) -> CorpusSummary:
    """Aggregate a list of SessionMetrics into a CorpusSummary."""
    s = CorpusSummary()
    for sess in sessions:
        if sess.turn_count == 0 and not sess.observations:
            continue
        s.total_files += 1
        s.duplicate_api_lines_skipped += sess.duplicate_api_lines_skipped
        t_count = sess.turn_count

        if sess.is_sidechain:
            s.subagent_sessions += 1
            s.subagent_turns += t_count
            s.subagent_cost_weighted += sess.cost_weighted_tokens()
        else:
            s.parent_sessions += 1
            s.parent_turns += t_count
            s.parent_cost_weighted += sess.cost_weighted_tokens()

        s.uncached_input += sess.uncached_input
        s.cache_write += sess.cache_write
        s.cache_write_5m += sess.cache_write_5m
        s.cache_write_1h += sess.cache_write_1h
        s.cache_read += sess.cache_read
        s.output += sess.output
        s.reasoning += sess.reasoning

        for obs in sess.observations:
            # Remaining turns in the trajectory that re-send this observation (at least 1)
            remaining_turns = max(1, t_count - obs.turn_index)
            resent = obs.tokens * remaining_turns

            s.calls_by_cat[obs.category] += 1
            s.first_tok_by_cat[obs.category] += obs.tokens
            s.resent_tok_by_cat[obs.category] += resent

            if obs.program:
                s.calls_by_prog[obs.program] += 1
                s.first_tok_by_prog[obs.program] += obs.tokens
                s.resent_tok_by_prog[obs.program] += resent

            if obs.category == "Read (tool)":
                s.read_sizes.append(obs.tokens)
                if obs.is_ranged_read:
                    s.read_ranged_count += 1
                else:
                    s.read_full_count += 1
                if obs.followed_search:
                    s.read_after_search_count += 1

    return s


def format_markdown_report(s: CorpusSummary, agent: str = "claude") -> str:
    """Render a comprehensive Markdown report of token economics and attribution."""
    w = DEFAULT_WEIGHTS.get(agent, DEFAULT_WEIGHTS["claude"])
    unaccounted_cw = max(0, s.cache_write - s.cache_write_5m - s.cache_write_1h)
    cw_cost = (
        (s.cache_write_5m + unaccounted_cw) * w["cache_write_5m"]
        + s.cache_write_1h * w["cache_write_1h"]
    )
    in_cost = s.uncached_input * w["uncached_input"]
    cr_cost = s.cache_read * w["cache_read"]
    out_cost = s.output * w["output"]
    tot_cost = max(1.0, in_cost + cw_cost + cr_cost + out_cost)
    tot_raw = max(1, s.total_raw_tokens)

    lines = [
        "# tokscope Session Token Economics Report",
        "",
        "## 1. Corpus Overview",
        "",
        f"- **Transcripts analyzed:** {s.total_files:,} ({s.parent_sessions:,} parent sessions, {s.subagent_sessions:,} sub-agent runs)",
        f"- **Deduplicated API turns:** {s.total_turns:,} ({s.parent_turns:,} parent, {s.subagent_turns:,} sub-agent; {s.duplicate_api_lines_skipped:,} duplicate streaming JSONL lines merged)",
        f"- **Prompt-cache hit rate:** **{100 * s.cache_hit_rate:.2f}%**",
        f"- **History amplification factor:** **{s.amplification_factor:.1f}×** (each new input token is re-sent ~{s.amplification_factor:.1f} times across the session)",
        f"- **Sub-agent cost share:** **{100 * s.subagent_cost_weighted / tot_cost:.1f}%** of total cost-weighted tokens ({100 * s.subagent_turns / max(1, s.total_turns):.1f}% of turns)",
        "",
        "## 2. Raw Tokens vs. Cost-Weighted Tokens",
        "",
        "*Cost-weighted tokens are expressed in uncached-input-token equivalents (`1.0×` = uncached input, `1.25×/2.0×` = 5m/1h cache write, `0.1×` = cache read, `5.0×` = output).*",
        "",
        "| Component | Raw Tokens | Share of Raw | Cost-Weighted Tokens | Share of Cost |",
        "|---|---:|---:|---:|---:|",
        f"| **Cache read (`0.1×`)** | {s.cache_read:,} | {100 * s.cache_read / tot_raw:.2f}% | {cr_cost:,.0f} | **{100 * cr_cost / tot_cost:.1f}%** |",
        f"| **Cache write (`1.25×` 5m / `2.0×` 1h)** | {s.cache_write:,} | {100 * s.cache_write / tot_raw:.2f}% | {cw_cost:,.0f} | **{100 * cw_cost / tot_cost:.1f}%** |",
        f"| **Output + thinking (`5.0×`)** | {s.output:,} | {100 * s.output / tot_raw:.2f}% | {out_cost:,.0f} | **{100 * out_cost / tot_cost:.1f}%** |",
        f"| **Uncached input (`1.0×`)** | {s.uncached_input:,} | {100 * s.uncached_input / tot_raw:.3f}% | {in_cost:,.0f} | **{100 * in_cost / tot_cost:.2f}%** |",
        f"| **Total** | **{s.total_raw_tokens:,}** | **100.0%** | **{tot_cost:,.0f}** | **100.0%** |",
        "",
        "## 3. Tool-Output Attribution (First-Occurrence vs. Re-sent History Exposure)",
        "",
        "| Category | Calls | First-Occurrence Tokens | Share (1×) | Re-sent Exposure ($\\sum (T-t)\\cdot\\text{tok}$) | Share (Re-sent) | Avg Amplification |",
        "|---|---:|---:|---:|---:|---:|---:|",
    ]

    tot_first = max(1, sum(s.first_tok_by_cat.values()))
    tot_resent = max(1, sum(s.resent_tok_by_cat.values()))
    for cat, f_tok in s.first_tok_by_cat.most_common(12):
        calls = s.calls_by_cat[cat]
        r_tok = s.resent_tok_by_cat[cat]
        amp = r_tok / max(1, f_tok)
        lines.append(
            f"| {cat} | {calls:,} | {f_tok:,} | {100 * f_tok / tot_first:.1f}% | {r_tok:,} | **{100 * r_tok / tot_resent:.1f}%** | {amp:.1f}× |"
        )

    if s.first_tok_by_prog:
        lines.extend([
            "",
            "## 4. Shell Output by Program (Top 12)",
            "",
            "| Program | Calls | First-Occurrence Tokens | Share of Shell (1×) | Re-sent Exposure | Share of Shell (Re-sent) |",
            "|---|---:|---:|---:|---:|---:|",
        ])
        sh_first = max(1, sum(s.first_tok_by_prog.values()))
        sh_resent = max(1, sum(s.resent_tok_by_prog.values()))
        for prog, f_tok in s.first_tok_by_prog.most_common(12):
            calls = s.calls_by_prog[prog]
            r_tok = s.resent_tok_by_prog[prog]
            lines.append(
                f"| `{prog}` | {calls:,} | {f_tok:,} | {100 * f_tok / sh_first:.1f}% | {r_tok:,} | {100 * r_tok / sh_resent:.1f}% |"
            )

    if s.read_sizes:
        srt = sorted(s.read_sizes)
        p = lambda q: srt[min(len(srt) - 1, int(q * len(srt)))]
        top10 = sum(srt[int(0.9 * len(srt)):]) / max(1, sum(srt))
        lines.extend([
            "",
            "## 5. Read Tool Profile",
            "",
            f"- **Calls:** {len(srt):,} ({s.read_full_count:,} whole-file [{100 * s.read_full_count / len(srt):.1f}%], {s.read_ranged_count:,} ranged)",
            f"- **Tokens per read:** median {statistics.median(srt):,.0f}, p90 {p(0.9):,}, p99 {p(0.99):,}, max {srt[-1]:,}",
            f"- **Heavy tail:** largest 10% of reads = **{100 * top10:.1f}%** of all `Read` tokens",
            f"- **Search-then-read pairs:** {s.read_after_search_count:,} reads immediately followed a search call",
        ])

    return "\n".join(lines) + "\n"
