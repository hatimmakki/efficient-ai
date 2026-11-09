"""Privacy-preserving export of session metrics (`tokscope sanitize`)."""

from __future__ import annotations

import json
from collections import Counter
from pathlib import Path

from .models import SessionMetrics


def sanitize_sessions(sessions: list[SessionMetrics]) -> list[dict]:
    """Convert SessionMetrics into privacy-safe per-session aggregate dictionaries.

    Contains ONLY hashed session IDs, agent type, turn counts, token totals,
    and allowlisted category counts. No file paths, prompts, commands, or code.
    """
    records: list[dict] = []
    for s in sessions:
        if s.turn_count == 0 and not s.observations:
            continue
        cat_tokens: Counter[str] = Counter()
        for obs in s.observations:
            cat_tokens[obs.category] += obs.tokens

        records.append({
            "session_id": s.session_id,
            "agent": s.agent,
            "is_sidechain": s.is_sidechain,
            "turn_count": s.turn_count,
            "uncached_input": s.uncached_input,
            "cache_write": s.cache_write,
            "cache_write_5m": s.cache_write_5m,
            "cache_write_1h": s.cache_write_1h,
            "cache_read": s.cache_read,
            "output": s.output,
            "reasoning": s.reasoning,
            "cache_hit_rate": round(s.cache_hit_rate, 4),
            "cost_weighted_tokens": round(s.cost_weighted_tokens(), 1),
            "tool_tokens_by_category": dict(cat_tokens),
        })
    return records


def write_sanitized_json(sessions: list[SessionMetrics], out_path: Path) -> int:
    records = sanitize_sessions(sessions)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(records, indent=2) + "\n", encoding="utf-8")
    return len(records)
