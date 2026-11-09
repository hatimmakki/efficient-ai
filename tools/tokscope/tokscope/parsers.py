"""Transcript parsers for Claude Code, Codex CLI, and Gemini CLI."""

from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path
from typing import Iterable

try:
    import tiktoken
    ENC = tiktoken.get_encoding("o200k_base")
except ImportError:
    ENC = None

from .models import SessionMetrics, ToolObservation, TurnUsage

SEARCH_PROGRAMS = {
    "rg", "grep", "egrep", "find", "fd", "ag", "ls", "tree",
    "select-string", "get-childitem", "git grep",
}
READ_PROGRAMS = {
    "cat", "head", "tail", "sed", "awk", "get-content",
}
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
CD_PREFIX_RE = re.compile(
    r"^\s*(?:cd|set-location|pushd)\s+(?:\"[^\"]*\"|'[^']*'|\S+)\s*(?:&&|;)\s*",
    re.I,
)


def count_tokens(text: str) -> int:
    if not text:
        return 0
    if ENC is not None:
        return len(ENC.encode(text, disallowed_special=()))
    # Fast BPE-calibrated fallback (~3.8 chars/token on code/logs) when tiktoken is not installed
    return max(1, int(len(text) / 3.8))


def anonymize_id(raw_id: str) -> str:
    return hashlib.sha256(raw_id.encode("utf-8", errors="ignore")).hexdigest()[:12]


def program_of(cmd: str) -> str:
    cmd = cmd or ""
    while m := CD_PREFIX_RE.match(cmd):
        cmd = cmd[m.end():]
    m = PROG_RE.match(cmd)
    if not m:
        return "other"
    prog = re.sub(r"\.exe$", "", m.group(1).lower())
    return prog if (prog in KNOWN_PROGRAMS or prog in SEARCH_PROGRAMS) else "other"


def extract_text(content: object) -> str:
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        parts: list[str] = []
        for item in content:
            if isinstance(item, dict):
                if "text" in item and isinstance(item["text"], str):
                    parts.append(item["text"])
                elif "content" in item:
                    parts.append(extract_text(item["content"]))
            elif isinstance(item, str):
                parts.append(item)
        return "\n".join(p for p in parts if p)
    if isinstance(content, dict):
        if "text" in content and isinstance(content["text"], str):
            return content["text"]
        if "output" in content and isinstance(content["output"], str):
            return content["output"]
    return str(content or "")


def classify_tool(name: str, inp: dict | None) -> tuple[str, str, bool]:
    """Return (category, program, is_ranged_read)."""
    inp = inp or {}
    lname = (name or "unknown").lower()
    if name in ("Bash", "PowerShell") or lname in ("shell", "exec_command", "run_shell_command", "bash"):
        cmd = inp.get("command") or inp.get("cmd") or ""
        if isinstance(cmd, list):
            cmd = " ".join(str(x) for x in cmd)
        prog = program_of(str(cmd))
        if prog in SEARCH_PROGRAMS:
            return "Search (shell)", prog, False
        if prog in READ_PROGRAMS:
            return "Read (shell: cat/sed/head)", prog, prog in ("sed", "head", "tail")
        return "Shell (other)", prog, False
    if name in ("Read", "read_file", "read_many_files", "view_file"):
        ranged = bool(
            inp.get("offset")
            or inp.get("limit")
            or inp.get("start_line")
            or inp.get("end_line")
        )
        return "Read (tool)", "", ranged
    if name in ("Grep", "Glob", "grep_search", "find_by_name", "search_files"):
        return "Search (Grep/Glob)", "", False
    if name.startswith("mcp__") or lname.startswith("mcp_"):
        return "MCP", "", False
    if name in ("Edit", "Write", "MultiEdit", "apply_patch", "replace", "write_file"):
        return "Edit/Write", "", False
    if name in ("Agent", "Task", "invoke_subagent"):
        return "Sub-agent result", "", False
    if name in ("WebSearch", "WebFetch", "google_web_search", "web_fetch"):
        return name, "", False
    return name or "Other", "", False


def parse_claude_jsonl(path: Path) -> SessionMetrics:
    """Parse a Claude Code session or subagent JSONL transcript."""
    is_sub_file = "subagents" in path.parts
    session = SessionMetrics(
        session_id=anonymize_id(str(path)),
        agent="claude",
        is_sidechain=is_sub_file,
    )

    # First pass: group assistant API turns by message.id (or requestId) so split
    # streaming blocks are merged and the final message_delta usage is used.
    turn_id_to_idx: dict[str, int] = {}
    tool_meta: dict[str, tuple[int, str, str, bool, bool]] = {}
    last_cat: str | None = None
    current_turn_idx = 0

    with path.open(encoding="utf-8", errors="ignore") as f_in:
        lines = f_in.readlines()

    for raw_line in lines:
        raw_line = raw_line.strip()
        if not raw_line:
            continue
        try:
            obj = json.loads(raw_line)
        except json.JSONDecodeError:
            continue

        rec_type = obj.get("type")
        if obj.get("isSidechain"):
            session.is_sidechain = True

        if rec_type == "assistant":
            msg = obj.get("message") or {}
            if not isinstance(msg, dict):
                continue
            model = str(msg.get("model") or "unknown")
            if model == "<synthetic>":
                continue

            mid = str(msg.get("id") or obj.get("requestId") or obj.get("uuid") or f"turn-{current_turn_idx}")
            usage = msg.get("usage") or {}

            if isinstance(usage, dict) and usage:
                inp = int(usage.get("input_tokens") or 0)
                cw = int(usage.get("cache_creation_input_tokens") or 0)
                cc = usage.get("cache_creation") or {}
                cw5 = int(cc.get("ephemeral_5m_input_tokens") or 0) if isinstance(cc, dict) else 0
                cw1h = int(cc.get("ephemeral_1h_input_tokens") or 0) if isinstance(cc, dict) else 0
                cr = int(usage.get("cache_read_input_tokens") or 0)
                out = int(usage.get("output_tokens") or 0)
                details = usage.get("output_tokens_details") or {}
                think = int(details.get("thinking_tokens") or 0) if isinstance(details, dict) else 0

                if mid in turn_id_to_idx:
                    session.duplicate_api_lines_skipped += 1
                    idx = turn_id_to_idx[mid]
                    prev = session.turns[idx]
                    # Keep the highest/latest usage across split streaming lines
                    prev.uncached_input = max(prev.uncached_input, inp)
                    prev.cache_write = max(prev.cache_write, cw)
                    prev.cache_write_5m = max(prev.cache_write_5m, cw5)
                    prev.cache_write_1h = max(prev.cache_write_1h, cw1h)
                    prev.cache_read = max(prev.cache_read, cr)
                    prev.output = max(prev.output, out)
                    prev.reasoning = max(prev.reasoning, think)
                else:
                    idx = len(session.turns)
                    turn_id_to_idx[mid] = idx
                    session.turns.append(
                        TurnUsage(
                            turn_index=idx,
                            turn_id=mid,
                            model=model,
                            is_sidechain=session.is_sidechain,
                            uncached_input=inp,
                            cache_write=cw,
                            cache_write_5m=cw5,
                            cache_write_1h=cw1h,
                            cache_read=cr,
                            output=out,
                            reasoning=think,
                        )
                    )
                    current_turn_idx = idx

            content = msg.get("content")
            if isinstance(content, list):
                for block in content:
                    if not isinstance(block, dict):
                        continue
                    btype = block.get("type")
                    if btype == "tool_use":
                        tname = str(block.get("name") or "unknown")
                        tinp = block.get("input") if isinstance(block.get("input"), dict) else {}
                        cat, prog, ranged = classify_tool(tname, tinp)
                        followed_search = bool(
                            cat.startswith("Read") and last_cat and last_cat.startswith("Search")
                        )
                        tid = str(block.get("id") or "")
                        if tid:
                            tool_meta[tid] = (current_turn_idx, cat, prog, ranged, followed_search)
                        last_cat = cat
                    elif btype == "text":
                        txt_tok = count_tokens(str(block.get("text") or ""))
                        if txt_tok > 0:
                            session.assistant_text_tokens.append((current_turn_idx, txt_tok))

        elif rec_type == "user":
            msg = obj.get("message") or {}
            content = msg.get("content") if isinstance(msg, dict) else None
            if isinstance(content, str):
                utok = count_tokens(content)
                if utok > 0:
                    session.user_prompt_tokens.append((current_turn_idx, utok))
            elif isinstance(content, list):
                for block in content:
                    if not isinstance(block, dict):
                        continue
                    btype = block.get("type")
                    if btype == "tool_result":
                        tid = str(block.get("tool_use_id") or "")
                        t_idx, cat, prog, ranged, followed_search = tool_meta.get(
                            tid, (current_turn_idx, "Unknown", "", False, False)
                        )
                        toks = count_tokens(extract_text(block.get("content")))
                        session.observations.append(
                            ToolObservation(
                                turn_index=t_idx,
                                category=cat,
                                program=prog,
                                tokens=toks,
                                is_ranged_read=ranged,
                                followed_search=followed_search,
                            )
                        )
                    elif btype == "text":
                        utok = count_tokens(str(block.get("text") or ""))
                        if utok > 0:
                            session.user_prompt_tokens.append((current_turn_idx, utok))

    return session


def parse_codex_jsonl(path: Path) -> SessionMetrics:
    """Parse a Codex CLI rollout-*.jsonl transcript."""
    session = SessionMetrics(
        session_id=anonymize_id(str(path)),
        agent="codex",
        is_sidechain=False,
    )
    prev_cumulative: tuple[int, int, int, int] | None = None
    call_meta: dict[str, tuple[int, str, str, bool, bool]] = {}
    last_cat: str | None = None
    model = "codex"

    with path.open(encoding="utf-8", errors="ignore") as f_in:
        lines = f_in.readlines()

    for raw_line in lines:
        raw_line = raw_line.strip()
        if not raw_line:
            continue
        try:
            obj = json.loads(raw_line)
        except json.JSONDecodeError:
            continue

        rtype = obj.get("type")
        payload = obj.get("payload") if isinstance(obj.get("payload"), dict) else obj

        if rtype == "turn_context" or payload.get("type") == "turn_context":
            model = str(payload.get("model") or model)

        # Codex token_count event
        if rtype == "token_count" or (rtype == "event_msg" and payload.get("type") == "token_count"):
            info = payload.get("info") if isinstance(payload.get("info"), dict) else payload
            if not isinstance(info, dict):
                continue
            tot_u = info.get("total_token_usage") or {}
            last_u = info.get("last_token_usage") or {}
            if isinstance(tot_u, dict) and tot_u:
                cum_sig = (
                    int(tot_u.get("input_tokens") or 0),
                    int(tot_u.get("cached_input_tokens") or 0),
                    int(tot_u.get("output_tokens") or 0),
                    int(tot_u.get("reasoning_output_tokens") or 0),
                )
                if cum_sig == prev_cumulative:
                    session.duplicate_api_lines_skipped += 1
                    continue
                if isinstance(last_u, dict) and last_u:
                    raw_in = int(last_u.get("input_tokens") or 0)
                    cached_in = int(last_u.get("cached_input_tokens") or 0)
                    out = int(last_u.get("output_tokens") or 0)
                    reas = int(last_u.get("reasoning_output_tokens") or 0)
                elif prev_cumulative is not None:
                    raw_in = max(0, cum_sig[0] - prev_cumulative[0])
                    cached_in = max(0, cum_sig[1] - prev_cumulative[1])
                    out = max(0, cum_sig[2] - prev_cumulative[2])
                    reas = max(0, cum_sig[3] - prev_cumulative[3])
                else:
                    raw_in, cached_in, out, reas = cum_sig
                prev_cumulative = cum_sig
            elif isinstance(last_u, dict) and last_u:
                raw_in = int(last_u.get("input_tokens") or 0)
                cached_in = int(last_u.get("cached_input_tokens") or 0)
                out = int(last_u.get("output_tokens") or 0)
                reas = int(last_u.get("reasoning_output_tokens") or 0)
            else:
                continue

            # OpenAI accounting: cached_input_tokens is a SUBSET of input_tokens
            uncached = max(0, raw_in - cached_in)
            idx = len(session.turns)
            session.turns.append(
                TurnUsage(
                    turn_index=idx,
                    turn_id=f"codex-turn-{idx}",
                    model=model,
                    uncached_input=uncached,
                    cache_write=0,
                    cache_read=cached_in,
                    output=out,
                    reasoning=reas,
                )
            )

        # Tool calls in Codex response_item
        elif rtype == "response_item" or payload.get("type") in ("function_call", "function_call_output"):
            ptype = payload.get("type")
            cur_idx = max(0, len(session.turns) - 1)
            if ptype in ("function_call", "custom_tool_call"):
                tname = str(payload.get("name") or "shell")
                raw_args = payload.get("arguments") or payload.get("input") or "{}"
                if isinstance(raw_args, str):
                    try:
                        tinp = json.loads(raw_args)
                    except json.JSONDecodeError:
                        tinp = {"command": raw_args}
                elif isinstance(raw_args, dict):
                    tinp = raw_args
                else:
                    tinp = {}
                cat, prog, ranged = classify_tool(tname, tinp)
                followed_search = bool(
                    cat.startswith("Read") and last_cat and last_cat.startswith("Search")
                )
                cid = str(payload.get("call_id") or payload.get("id") or "")
                if cid:
                    call_meta[cid] = (cur_idx, cat, prog, ranged, followed_search)
                last_cat = cat
            elif ptype in ("function_call_output", "custom_tool_call_output"):
                cid = str(payload.get("call_id") or payload.get("id") or "")
                t_idx, cat, prog, ranged, followed_search = call_meta.get(
                    cid, (cur_idx, "Shell (other)", "", False, False)
                )
                out_text = extract_text(payload.get("output"))
                session.observations.append(
                    ToolObservation(
                        turn_index=t_idx,
                        category=cat,
                        program=prog,
                        tokens=count_tokens(out_text),
                        is_ranged_read=ranged,
                        followed_search=followed_search,
                    )
                )

    return session


def parse_gemini_json(path: Path) -> SessionMetrics:
    """Parse a Gemini CLI chat JSON transcript."""
    session = SessionMetrics(
        session_id=anonymize_id(str(path)),
        agent="gemini",
        is_sidechain=False,
    )
    try:
        data = json.loads(path.read_text(encoding="utf-8", errors="ignore"))
    except json.JSONDecodeError:
        return session

    messages = data.get("messages") if isinstance(data, dict) else (data if isinstance(data, list) else [])
    if not isinstance(messages, list):
        return session

    last_cat: str | None = None
    for msg in messages:
        if not isinstance(msg, dict):
            continue
        role = str(msg.get("type") or msg.get("role") or "")
        um = msg.get("usageMetadata") or msg.get("tokens") or {}
        if isinstance(um, dict) and um:
            prompt = int(um.get("promptTokenCount") or um.get("input") or 0)
            cached = int(um.get("cachedContentTokenCount") or um.get("cached") or 0)
            cand = int(um.get("candidatesTokenCount") or um.get("output") or 0)
            thoughts = int(um.get("thoughtsTokenCount") or um.get("thoughts") or 0)
            # Gemini accounting: cachedContentTokenCount is a SUBSET of promptTokenCount
            uncached = max(0, prompt - cached)
            idx = len(session.turns)
            session.turns.append(
                TurnUsage(
                    turn_index=idx,
                    turn_id=str(msg.get("id") or f"gemini-turn-{idx}"),
                    model=str(msg.get("model") or "gemini"),
                    uncached_input=uncached,
                    cache_write=0,
                    cache_read=cached,
                    output=cand + thoughts,
                    reasoning=thoughts,
                )
            )

        cur_idx = max(0, len(session.turns) - 1)
        tool_calls = msg.get("toolCalls") or []
        if isinstance(tool_calls, list):
            for tc in tool_calls:
                if not isinstance(tc, dict):
                    continue
                tname = str(tc.get("name") or "unknown")
                tinp = tc.get("args") if isinstance(tc.get("args"), dict) else {}
                cat, prog, ranged = classify_tool(tname, tinp)
                followed_search = bool(
                    cat.startswith("Read") and last_cat and last_cat.startswith("Search")
                )
                last_cat = cat
                res = tc.get("result") or tc.get("response") or ""
                if res:
                    session.observations.append(
                        ToolObservation(
                            turn_index=cur_idx,
                            category=cat,
                            program=prog,
                            tokens=count_tokens(extract_text(res)),
                            is_ranged_read=ranged,
                            followed_search=followed_search,
                        )
                    )

        if role in ("user", "human"):
            utok = count_tokens(extract_text(msg.get("content") or msg.get("parts")))
            if utok > 0:
                session.user_prompt_tokens.append((cur_idx, utok))

    return session


def detect_and_parse(path: Path, agent: str = "auto") -> SessionMetrics | None:
    """Detect transcript agent type and parse into SessionMetrics."""
    if not path.is_file():
        return None
    if agent == "claude":
        return parse_claude_jsonl(path)
    if agent == "codex":
        return parse_codex_jsonl(path)
    if agent == "gemini":
        return parse_gemini_json(path)

    # Auto-detect
    suffix = path.suffix.lower()
    name = path.name.lower()
    parts_lower = {p.lower() for p in path.parts}

    if suffix == ".json":
        if ".gemini" in parts_lower or "gemini" in name or "chat" in name:
            return parse_gemini_json(path)
        return None

    if suffix == ".jsonl":
        if ".codex" in parts_lower or name.startswith("rollout-"):
            return parse_codex_jsonl(path)
        if ".claude" in parts_lower:
            return parse_claude_jsonl(path)
        # Peek at first non-empty line
        try:
            with path.open(encoding="utf-8", errors="ignore") as f:
                for line in f:
                    line = line.strip()
                    if not line:
                        continue
                    obj = json.loads(line)
                    if "event_msg" in line or obj.get("type") in ("session_meta", "turn_context", "token_count"):
                        return parse_codex_jsonl(path)
                    return parse_claude_jsonl(path)
        except Exception:
            return None
    return None


def iter_session_files(root: Path) -> Iterable[Path]:
    """Yield transcript files under root (or root itself if a file)."""
    if root.is_file():
        yield root
        return
    if not root.exists():
        return
    for p in sorted(root.rglob("*")):
        if p.is_file() and p.suffix.lower() in (".jsonl", ".json"):
            yield p
