"""Experiment 007: Cache-Aware Batch Observation Masking (H1).

Simulates prompt-cache prefix invalidation and total session cost across:
1. Baseline (No Masking)
2. Naive Per-Turn Sliding Window Masking (window M=8, mutates prefix every turn t > M)
3. Periodic LLM Summarization (every K=15 turns, +summary call + 15% trajectory elongation per arXiv:2508.21433)
4. Cache-Aware Batch Observation Masking (H1: compact old observations >M turns old only every K turns,
   keeping turns 1..b_{k-1} prefix-frozen across compactions).

Evaluated on:
- Part A: Controlled 40-turn SWE-agent coding trajectory.
- Part B: Real multi-turn trajectories (>= 15 turns) from the 315-session Claude Code corpus.
"""

from __future__ import annotations

import sys
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(REPO_ROOT / "tools" / "tokscope"))

from tokscope.parsers import detect_and_parse, iter_session_files  # noqa: E402

PLACEHOLDER_TOKENS = 15  # `[Observation elided; full log: .eai/logs/turn_k.log]`


@dataclass
class SimResult:
    policy: str
    raw_input_tokens: int
    cache_read_tokens: int
    cache_write_tokens: int
    output_tokens: int
    cache_hit_rate: float
    claude_cost_eq: float  # 1.25x write, 0.10x read, 5.0x output
    codex_cost_eq: float   # 1.00x write/uncached, 0.50x read, 4.0x output


def simulate_trajectory(
    static_prefix: int,
    obs_per_turn: list[int],
    out_per_turn: list[int],
    policy: str,
    batch_k: int = 10,
    keep_recent_m: int = 6,
    min_mask_tokens: int = 120,
) -> SimResult:
    """Simulate exact prefix byte-stability and cache read/write tokens across T turns.

    At each turn t (0 .. T-1):
    Context consists of: [static_prefix, turn_0_block, turn_1_block, ..., turn_t_block]
    where turn_i_block = (current_obs_i + out_i).
    Prompt cache matches the longest identical prefix of blocks from turn t-1!
    Any block i whose observation token size changes between turn t-1 and turn t
    invalidates the cache from block i onward for turn t.
    """
    n_turns = len(obs_per_turn)
    # Current observation size in history for each turn i
    cur_obs = list(obs_per_turn)
    prev_blocks: list[int] = []  # Block sizes sent on turn t-1

    total_raw_in = 0
    total_cache_read = 0
    total_cache_write = 0
    total_out = 0

    for t in range(n_turns):
        # Apply masking policy before sending turn t
        if policy == "naive_sliding":
            # Mask any turn i <= t - keep_recent_m on EVERY turn
            cutoff = t - keep_recent_m
            for i in range(max(0, cutoff)):
                if cur_obs[i] >= min_mask_tokens:
                    cur_obs[i] = PLACEHOLDER_TOKENS

        elif policy == "batch_masking":
            # Only mask when t > keep_recent_m and (t % batch_k == 0)
            if t > keep_recent_m and (t % batch_k == 0):
                cutoff = t - keep_recent_m
                for i in range(max(0, cutoff)):
                    if cur_obs[i] >= min_mask_tokens:
                        cur_obs[i] = PLACEHOLDER_TOKENS

        elif policy == "llm_summary":
            # Every batch_k turns, summarize everything up to t - keep_recent_m into one 800-token summary block
            if t > keep_recent_m and (t % batch_k == 0):
                cutoff = t - keep_recent_m
                # Pay for the LLM summarization call itself (reads current prefix + outputs 800 tokens)
                sum_in = static_prefix + sum(prev_blocks)
                total_raw_in += sum_in
                total_cache_read += sum_in
                total_out += 800
                # Replace first non-zero block with 800, zero out the rest up to cutoff
                for i in range(max(0, cutoff)):
                    cur_obs[i] = 800 if i == 0 else 0

        # Build current context blocks for turn t: [static_prefix, block_0, ..., block_t]
        cur_blocks = [static_prefix]
        for i in range(t):
            cur_blocks.append(cur_obs[i] + out_per_turn[i])
        # Turn t's new input (user/tool result entering at turn t)
        cur_blocks.append(cur_obs[t])

        # Longest common prefix between prev_blocks and cur_blocks is a cache HIT
        cached_tok = 0
        for prev_b, cur_b in zip(prev_blocks, cur_blocks):
            if prev_b == cur_b:
                cached_tok += cur_b
            else:
                break

        turn_in = sum(cur_blocks)
        write_tok = turn_in - cached_tok

        total_raw_in += turn_in
        total_cache_read += cached_tok
        total_cache_write += write_tok
        total_out += out_per_turn[t]

        # After turn t completes, assistant output out_per_turn[t] is appended to block t for next turn
        prev_blocks = list(cur_blocks)
        prev_blocks[-1] += out_per_turn[t]

    hit_rate = total_cache_read / max(1, total_raw_in)
    claude_cost = total_cache_write * 1.25 + total_cache_read * 0.10 + total_out * 5.0
    codex_cost = total_cache_write * 1.00 + total_cache_read * 0.50 + total_out * 4.0

    return SimResult(
        policy=policy,
        raw_input_tokens=total_raw_in,
        cache_read_tokens=total_cache_read,
        cache_write_tokens=total_cache_write,
        output_tokens=total_out,
        cache_hit_rate=hit_rate,
        claude_cost_eq=claude_cost,
        codex_cost_eq=codex_cost,
    )


def run_controlled_40_turn_benchmark() -> None:
    print("## 1. Controlled 40-Turn Coding Trajectory (Static Prefix $S = 6,000$, Mean Obs $= 1,800$ tok/turn)\n")
    static_prefix = 6000
    # Realistic coding pattern: alternating larger file reads (3,200 tok) and smaller search/edit/test results (400 tok)
    obs = [3200 if (i % 2 == 0) else 400 for i in range(40)]
    out = [280 for _ in range(40)]

    configs = [
        ("1. Baseline (No Masking)", "baseline", 10, 6),
        ("2. Naive Per-Turn Sliding Mask ($M=6$, every turn)", "naive_sliding", 1, 6),
        ("3. Periodic LLM Summarization ($K=12, M=4$)", "llm_summary", 12, 4),
        ("4. **H1 Batch Masking ($K=5, M=6$)**", "batch_masking", 5, 6),
        ("5. **H1 Batch Masking ($K=8, M=6$)**", "batch_masking", 8, 6),
        ("6. **H1 Batch Masking ($K=10, M=6$)**", "batch_masking", 10, 6),
        ("7. **H1 Batch Masking ($K=15, M=6$)**", "batch_masking", 15, 6),
    ]

    base = simulate_trajectory(static_prefix, obs, out, "baseline")

    print("| Strategy | Raw Input Tokens | Δ Raw Input | Cache Hit Rate | Cache Writes | Claude Cost (`1×` eq.) | Δ Claude Cost | Codex Cost (`1×` eq.) | Δ Codex Cost |")
    print("|---|---:|---:|---:|---:|---:|---:|---:|---:|")

    for label, pol, k, m in configs:
        res = simulate_trajectory(static_prefix, obs, out, pol, batch_k=k, keep_recent_m=m)
        d_raw = 100.0 * (res.raw_input_tokens - base.raw_input_tokens) / base.raw_input_tokens
        d_cl = 100.0 * (res.claude_cost_eq - base.claude_cost_eq) / base.claude_cost_eq
        d_cx = 100.0 * (res.codex_cost_eq - base.codex_cost_eq) / base.codex_cost_eq
        print(
            f"| {label} | {res.raw_input_tokens:,} | {d_raw:+.1f}% | {100*res.cache_hit_rate:.1f}% | {res.cache_write_tokens:,} | {res.claude_cost_eq:,.0f} | **{d_cl:+.1f}%** | {res.codex_cost_eq:,.0f} | **{d_cx:+.1f}%** |"
        )
    print()


def run_corpus_replay() -> None:
    claude_root = Path.home() / ".claude" / "projects"
    if not claude_root.exists():
        return

    long_sessions = []
    for fpath in iter_session_files(claude_root):
        s = detect_and_parse(fpath, agent="claude")
        if s and s.turn_count >= 15 and len(s.observations) >= 10:
            long_sessions.append(s)

    print(f"## 2. Replay Across {len(long_sessions)} Real Multi-Turn Sessions ($T \\ge 15$ turns, {sum(s.turn_count for s in long_sessions):,} total turns)\n")

    configs = [
        ("1. Baseline (No Masking)", "baseline", 10, 6),
        ("2. Naive Per-Turn Sliding Mask ($M=6$, every turn)", "naive_sliding", 1, 6),
        ("3. Periodic LLM Summarization ($K=12, M=4$)", "llm_summary", 12, 4),
        ("4. **H1 Batch Masking ($K=8, M=6$)**", "batch_masking", 8, 6),
        ("5. **H1 Batch Masking ($K=10, M=6$)**", "batch_masking", 10, 6),
        ("6. **H1 Batch Masking ($K=15, M=6$)**", "batch_masking", 15, 6),
    ]

    totals: dict[str, list[float]] = {c[0]: [0.0, 0.0, 0.0, 0.0, 0.0] for c in configs}

    for s in long_sessions:
        t_len = s.turn_count
        obs_by_t = [50 for _ in range(t_len)]
        for ob in s.observations:
            if 0 <= ob.turn_index < t_len:
                obs_by_t[ob.turn_index] += ob.tokens
        out_by_t = [max(40, t.output) for t in s.turns]
        static_p = max(4000, min(20000, s.turns[0].total_input))

        for label, pol, k, m in configs:
            r = simulate_trajectory(static_p, obs_by_t, out_by_t, pol, batch_k=k, keep_recent_m=m)
            acc = totals[label]
            acc[0] += r.raw_input_tokens
            acc[1] += r.cache_read_tokens
            acc[2] += r.cache_write_tokens
            acc[3] += r.claude_cost_eq
            acc[4] += r.codex_cost_eq

    base_acc = totals["1. Baseline (No Masking)"]

    print("| Strategy | Raw Input Tokens | Δ Raw Input | Cache Hit Rate | Cache Writes | Claude Cost (`1×` eq.) | Δ Claude Cost | Codex Cost (`1×` eq.) | Δ Codex Cost |")
    print("|---|---:|---:|---:|---:|---:|---:|---:|---:|")
    for label, _, _, _ in configs:
        raw_in, cr, cw, cl_c, cx_c = totals[label]
        hit = cr / max(1.0, raw_in)
        d_raw = 100.0 * (raw_in - base_acc[0]) / base_acc[0]
        d_cl = 100.0 * (cl_c - base_acc[3]) / base_acc[3]
        d_cx = 100.0 * (cx_c - base_acc[4]) / base_acc[4]
        print(
            f"| {label} | {int(raw_in):,} | {d_raw:+.1f}% | {100*hit:.1f}% | {int(cw):,} | {cl_c:,.0f} | **{d_cl:+.1f}%** | {cx_c:,.0f} | **{d_cx:+.1f}%** |"
        )


def main() -> None:
    print("# Experiment 007: Cache-Aware Batch Observation Masking (H1)\n")
    run_controlled_40_turn_benchmark()
    run_corpus_replay()


if __name__ == "__main__":
    main()
