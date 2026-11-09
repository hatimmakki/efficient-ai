"""Normalized usage, attribution, and cost models across Claude Code, Codex CLI, and Gemini CLI."""

from __future__ import annotations

from dataclasses import dataclass, field


# Cost weights normalized to 1.0 = uncached base input token price.
# Anthropic standard ratios:
#   uncached input = 1.0x
#   cache write (5m TTL) = 1.25x
#   cache write (1h TTL) = 2.0x
#   cache read = 0.1x
#   output (including thinking/reasoning) = 5.0x
# OpenAI / Gemini standard ratios:
#   uncached input = 1.0x
#   cache read = 0.5x (OpenAI) or 0.25x (Gemini)
#   output = 4.0x - 5.0x
DEFAULT_WEIGHTS = {
    "claude": {
        "uncached_input": 1.0,
        "cache_write_5m": 1.25,
        "cache_write_1h": 2.0,
        "cache_read": 0.10,
        "output": 5.0,
    },
    "codex": {
        "uncached_input": 1.0,
        "cache_write_5m": 1.0,
        "cache_write_1h": 1.0,
        "cache_read": 0.50,
        "output": 4.0,
    },
    "gemini": {
        "uncached_input": 1.0,
        "cache_write_5m": 1.0,
        "cache_write_1h": 1.0,
        "cache_read": 0.25,
        "output": 4.0,
    },
}


@dataclass
class TurnUsage:
    """Normalized token accounting for a single API response turn."""

    turn_index: int
    turn_id: str
    model: str = "unknown"
    is_sidechain: bool = False
    uncached_input: int = 0
    cache_write: int = 0
    cache_write_5m: int = 0
    cache_write_1h: int = 0
    cache_read: int = 0
    output: int = 0
    reasoning: int = 0

    @property
    def total_input(self) -> int:
        """Total context tokens processed on this turn (uncached + cache_write + cache_read)."""
        return self.uncached_input + self.cache_write + self.cache_read

    @property
    def new_input(self) -> int:
        """Tokens newly entering the context or missed by cache on this turn."""
        return self.uncached_input + self.cache_write

    @property
    def total_tokens(self) -> int:
        """Total raw tokens (context + output)."""
        return self.total_input + self.output

    def cost_weighted_tokens(self, agent: str = "claude") -> float:
        """Cost in input-token-equivalent units (where 1.0 = 1 uncached input token)."""
        w = DEFAULT_WEIGHTS.get(agent, DEFAULT_WEIGHTS["claude"])
        # If 5m/1h split is populated, use exact split; otherwise treat cache_write as 5m TTL.
        if self.cache_write_5m or self.cache_write_1h:
            unaccounted = max(0, self.cache_write - self.cache_write_5m - self.cache_write_1h)
            cw_cost = (
                (self.cache_write_5m + unaccounted) * w["cache_write_5m"]
                + self.cache_write_1h * w["cache_write_1h"]
            )
        else:
            cw_cost = self.cache_write * w["cache_write_5m"]
        return (
            self.uncached_input * w["uncached_input"]
            + cw_cost
            + self.cache_read * w["cache_read"]
            + self.output * w["output"]
        )


@dataclass
class ToolObservation:
    """A single tool execution result observed at a specific turn in a trajectory."""

    turn_index: int
    category: str
    program: str = ""
    tokens: int = 0
    is_ranged_read: bool = False
    followed_search: bool = False


@dataclass
class SessionMetrics:
    """Aggregated metrics for one transcript file (parent session or subagent trajectory)."""

    session_id: str
    agent: str
    is_sidechain: bool = False
    turns: list[TurnUsage] = field(default_factory=list)
    observations: list[ToolObservation] = field(default_factory=list)
    user_prompt_tokens: list[tuple[int, int]] = field(default_factory=list)  # (turn_index, tokens)
    assistant_text_tokens: list[tuple[int, int]] = field(default_factory=list)  # (turn_index, tokens)
    duplicate_api_lines_skipped: int = 0

    @property
    def turn_count(self) -> int:
        return len(self.turns)

    @property
    def uncached_input(self) -> int:
        return sum(t.uncached_input for t in self.turns)

    @property
    def cache_write(self) -> int:
        return sum(t.cache_write for t in self.turns)

    @property
    def cache_write_5m(self) -> int:
        return sum(t.cache_write_5m for t in self.turns)

    @property
    def cache_write_1h(self) -> int:
        return sum(t.cache_write_1h for t in self.turns)

    @property
    def cache_read(self) -> int:
        return sum(t.cache_read for t in self.turns)

    @property
    def total_input(self) -> int:
        return sum(t.total_input for t in self.turns)

    @property
    def output(self) -> int:
        return sum(t.output for t in self.turns)

    @property
    def reasoning(self) -> int:
        return sum(t.reasoning for t in self.turns)

    @property
    def cache_hit_rate(self) -> float:
        tot = self.total_input
        return (self.cache_read / tot) if tot > 0 else 0.0

    @property
    def history_amplification_factor(self) -> float:
        """Ratio of total context tokens sent across turns to new tokens written."""
        new_tok = self.uncached_input + self.cache_write
        return (self.total_input / new_tok) if new_tok > 0 else 1.0

    def cost_weighted_tokens(self) -> float:
        return sum(t.cost_weighted_tokens(self.agent) for t in self.turns)
