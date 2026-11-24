//! Multi-agent session token attribution, USD cost estimation, static-prefix auditing,
//! privacy-safe sanitization, and fixture self-tests for:
//! Claude Code, Antigravity, Codex CLI, and Gemini CLI.

use crate::telemetry::{est_tokens_from_chars, user_home};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub struct AgentWeights {
    pub uncached_input: f64,
    pub cache_write_5m: f64,
    pub cache_write_1h: f64,
    pub cache_read: f64,
    pub output: f64,
}

pub fn weights_for_agent(agent: &str) -> AgentWeights {
    match agent {
        "claude" => AgentWeights {
            uncached_input: 1.0,
            cache_write_5m: 1.25,
            cache_write_1h: 2.0,
            cache_read: 0.10,
            output: 5.0,
        },
        "codex" => AgentWeights {
            uncached_input: 1.0,
            cache_write_5m: 1.0,
            cache_write_1h: 1.0,
            cache_read: 0.50,
            output: 4.0,
        },
        "gemini" | "antigravity" => AgentWeights {
            uncached_input: 1.0,
            cache_write_5m: 1.0,
            cache_write_1h: 1.0,
            cache_read: 0.25,
            output: 4.0,
        },
        _ => weights_for_agent("claude"),
    }
}

/// Base uncached input price in USD per 1M tokens (used with provider multipliers for cost estimates).
pub fn base_input_usd_per_m(model: &str, agent: &str) -> f64 {
    let m = model.to_ascii_lowercase();
    if m.contains("opus") {
        15.0
    } else if m.contains("haiku") {
        0.80
    } else if m.contains("sonnet") || agent == "claude" {
        3.0
    } else if m.contains("flash-lite") || m.contains("flash_lite") {
        0.10
    } else if m.contains("flash") {
        0.30
    } else if m.contains("gemini") || agent == "gemini" || agent == "antigravity" {
        1.25
    } else if m.contains("mini") || m.contains("nano") {
        0.25
    } else if m.contains("codex") || m.contains("gpt") || agent == "codex" {
        1.50
    } else {
        2.0
    }
}

#[derive(Debug, Clone)]
pub struct TurnUsage {
    pub model: String,
    pub uncached_input: u64,
    pub cache_write: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
    pub cache_read: u64,
    pub output: u64,
    pub reasoning: u64,
}

impl TurnUsage {
    pub fn total_input(&self) -> u64 {
        self.uncached_input + self.cache_write + self.cache_read
    }

    pub fn cost_weighted(&self, agent: &str) -> f64 {
        let w = weights_for_agent(agent);
        let cw_cost = if self.cache_write_5m > 0 || self.cache_write_1h > 0 {
            let unaccounted = self
                .cache_write
                .saturating_sub(self.cache_write_5m + self.cache_write_1h);
            ((self.cache_write_5m + unaccounted) as f64) * w.cache_write_5m
                + (self.cache_write_1h as f64) * w.cache_write_1h
        } else {
            (self.cache_write as f64) * w.cache_write_5m
        };
        (self.uncached_input as f64) * w.uncached_input
            + cw_cost
            + (self.cache_read as f64) * w.cache_read
            + (self.output as f64) * w.output
    }

    pub fn estimated_usd(&self, agent: &str) -> f64 {
        let cw = self.cost_weighted(agent);
        let base_per_m = base_input_usd_per_m(&self.model, agent);
        (cw / 1_000_000.0) * base_per_m
    }
}

#[derive(Debug, Clone)]
pub struct ToolObservation {
    pub turn_index: usize,
    pub category: String,
    pub program: String,
    pub tokens: u64,
    pub is_ranged_read: bool,
    pub followed_search: bool,
}

#[derive(Debug, Clone)]
pub struct SessionMetrics {
    pub session_id: String,
    pub agent: String,
    pub is_sidechain: bool,
    pub mtime_secs: u64,
    pub turns: Vec<TurnUsage>,
    pub observations: Vec<ToolObservation>,
    pub duplicate_api_lines_skipped: usize,
}

/// Howard Hinnant's civil-from-days algorithm: convert Unix timestamp (seconds) to `YYYY-MM-DD` (UTC).
pub fn unix_secs_to_date(secs: u64) -> String {
    let z = (secs / 86_400) as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let yr = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", yr, m, d)
}

impl SessionMetrics {
    pub fn turn_count(&self) -> usize {
        self.turns.len()
    }
    pub fn uncached_input(&self) -> u64 {
        self.turns.iter().map(|t| t.uncached_input).sum()
    }
    pub fn cache_write(&self) -> u64 {
        self.turns.iter().map(|t| t.cache_write).sum()
    }
    pub fn cache_write_5m(&self) -> u64 {
        self.turns.iter().map(|t| t.cache_write_5m).sum()
    }
    pub fn cache_write_1h(&self) -> u64 {
        self.turns.iter().map(|t| t.cache_write_1h).sum()
    }
    pub fn cache_read(&self) -> u64 {
        self.turns.iter().map(|t| t.cache_read).sum()
    }
    pub fn total_input(&self) -> u64 {
        self.turns.iter().map(|t| t.total_input()).sum()
    }
    pub fn output(&self) -> u64 {
        self.turns.iter().map(|t| t.output).sum()
    }
    pub fn reasoning(&self) -> u64 {
        self.turns.iter().map(|t| t.reasoning).sum()
    }
    pub fn cache_hit_rate(&self) -> f64 {
        let tot = self.total_input();
        if tot == 0 {
            0.0
        } else {
            (self.cache_read() as f64) / (tot as f64)
        }
    }
    pub fn cost_weighted(&self) -> f64 {
        self.turns.iter().map(|t| t.cost_weighted(&self.agent)).sum()
    }
    pub fn estimated_usd(&self) -> f64 {
        self.turns.iter().map(|t| t.estimated_usd(&self.agent)).sum()
    }
}

fn anonymize_id(raw: &str) -> String {
    // 64-bit FNV-1a hex truncated to 12 chars (deterministic, zero private data)
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in raw.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:012x}", h & 0xffff_ffff_ffff)
}

fn count_tokens(text: &str) -> u64 {
    est_tokens_from_chars(text.len()) as u64
}

fn extract_text(val: &Value) -> String {
    match val {
        Value::String(s) => s.clone(),
        Value::Array(arr) => arr
            .iter()
            .filter_map(|item| match item {
                Value::String(s) => Some(s.clone()),
                Value::Object(map) => {
                    if let Some(Value::String(t)) = map.get("text") {
                        Some(t.clone())
                    } else {
                        map.get("content").map(extract_text)
                    }
                }
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(map) => {
            if let Some(Value::String(t)) = map.get("text") {
                t.clone()
            } else if let Some(Value::String(o)) = map.get("output") {
                o.clone()
            } else {
                String::new()
            }
        }
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn program_of(cmd: &str) -> String {
    let mut s = cmd.trim();
    // Strip simple `cd <dir> &&` prefixes
    for sep in ["&&", ";"] {
        if let Some(rest) = s.strip_prefix("cd ").or_else(|| s.strip_prefix("Set-Location ")) {
            if let Some(idx) = rest.find(sep) {
                s = rest[idx + sep.len()..].trim();
            }
        }
    }
    let mut parts = s.split_whitespace();
    let Some(mut first) = parts.next() else {
        return "other".to_string();
    };
    if first.eq_ignore_ascii_case("sudo") {
        first = parts.next().unwrap_or("other");
    }
    let lower = first.to_ascii_lowercase();
    let prog = lower.strip_suffix(".exe").unwrap_or(&lower);
    if prog == "git" && parts.next() == Some("grep") {
        return "git grep".to_string();
    }
    const KNOWN: &[&str] = &[
        "rg", "grep", "egrep", "find", "fd", "ag", "ls", "tree", "select-string", "get-childitem",
        "cat", "head", "tail", "sed", "awk", "get-content", "git", "gh", "npm", "npx", "pnpm",
        "yarn", "node", "bun", "deno", "python", "python3", "py", "pip", "uv", "pytest", "poetry",
        "cargo", "rustc", "go", "make", "cmake", "dotnet", "java", "mvn", "gradle", "docker",
        "kubectl", "curl", "wget", "jq", "tsc", "eslint", "ruff", "mypy", "flutter", "dart",
        "outline", "squeeze", "eai",
    ];
    if KNOWN.contains(&prog) {
        prog.to_string()
    } else {
        "other".to_string()
    }
}

fn classify_tool(name: &str, inp: Option<&Value>) -> (String, String, bool) {
    let lname = name.to_ascii_lowercase();
    let map = inp.and_then(|v| v.as_object());

    if matches!(
        lname.as_str(),
        "bash" | "powershell" | "shell" | "exec_command" | "run_shell_command" | "run_command"
    ) {
        let cmd_str = map
            .and_then(|m| {
                m.get("command")
                    .or_else(|| m.get("cmd"))
                    .or_else(|| m.get("CommandLine"))
            })
            .map(|v| match v {
                Value::String(s) => s.clone(),
                Value::Array(arr) => arr
                    .iter()
                    .filter_map(|x| x.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
                _ => String::new(),
            })
            .unwrap_or_default();
        let prog = program_of(&cmd_str);
        if matches!(
            prog.as_str(),
            "rg" | "grep" | "egrep" | "find" | "fd" | "ag" | "ls" | "tree" | "git grep"
                | "select-string" | "get-childitem"
        ) {
            return ("Search (shell)".into(), prog, false);
        }
        if matches!(
            prog.as_str(),
            "cat" | "head" | "tail" | "sed" | "awk" | "get-content"
        ) {
            let ranged = matches!(prog.as_str(), "sed" | "head" | "tail");
            return ("Read (shell: cat/sed/head)".into(), prog, ranged);
        }
        if matches!(prog.as_str(), "outline" | "eai") && cmd_str.contains("outline") {
            return ("Read (outline)".into(), prog, true);
        }
        return ("Shell (other)".into(), prog, false);
    }

    if matches!(
        lname.as_str(),
        "read" | "read_file" | "read_many_files" | "view_file"
    ) {
        let ranged = map
            .map(|m| {
                m.get("offset").is_some()
                    || m.get("limit").is_some()
                    || m.get("start_line").is_some()
                    || m.get("end_line").is_some()
                    || m.get("StartLine").is_some()
                    || m.get("EndLine").is_some()
            })
            .unwrap_or(false);
        return ("Read (tool)".into(), String::new(), ranged);
    }

    if matches!(
        lname.as_str(),
        "grep" | "glob" | "grep_search" | "find_by_name" | "search_files" | "list_dir"
    ) {
        return ("Search (Grep/Glob)".into(), String::new(), false);
    }
    if lname.starts_with("mcp__") || lname.starts_with("mcp_") || lname == "call_mcp_tool" {
        return ("MCP".into(), String::new(), false);
    }
    if matches!(
        lname.as_str(),
        "edit"
            | "write"
            | "multiedit"
            | "apply_patch"
            | "replace"
            | "write_file"
            | "edit_file"
            | "write_to_file"
            | "replace_file_content"
            | "multi_replace_file_content"
    ) {
        return ("Edit/Write".into(), String::new(), false);
    }
    if matches!(lname.as_str(), "agent" | "task" | "invoke_subagent") {
        return ("Sub-agent result".into(), String::new(), false);
    }
    if matches!(
        lname.as_str(),
        "websearch" | "webfetch" | "google_web_search" | "web_fetch" | "search_web"
    ) {
        return (name.to_string(), String::new(), false);
    }
    (
        if name.is_empty() { "Other" } else { name }.to_string(),
        String::new(),
        false,
    )
}

pub fn parse_claude_str(raw: &str, id_seed: &str, is_sub_file: bool) -> SessionMetrics {
    let mut session = SessionMetrics {
        session_id: anonymize_id(id_seed),
        agent: "claude".into(),
        is_sidechain: is_sub_file,
        mtime_secs: 0,
        turns: Vec::new(),
        observations: Vec::new(),
        duplicate_api_lines_skipped: 0,
    };

    let mut turn_id_to_idx: HashMap<String, usize> = HashMap::new();
    let mut tool_meta: HashMap<String, (usize, String, String, bool, bool)> = HashMap::new();
    let mut last_cat: Option<String> = None;
    let mut current_turn_idx = 0usize;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(obj) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        if obj.get("isSidechain").and_then(|v| v.as_bool()) == Some(true) {
            session.is_sidechain = true;
        }
        let rec_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");

        if rec_type == "assistant" {
            let Some(msg) = obj.get("message").and_then(|v| v.as_object()) else {
                continue;
            };
            let model = msg
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            if model == "<synthetic>" {
                continue;
            }
            let mid = msg
                .get("id")
                .and_then(|v| v.as_str())
                .or_else(|| obj.get("requestId").and_then(|v| v.as_str()))
                .or_else(|| obj.get("uuid").and_then(|v| v.as_str()))
                .map(str::to_string)
                .unwrap_or_else(|| format!("turn-{}", current_turn_idx));

            if let Some(usage) = msg.get("usage").and_then(|v| v.as_object()) {
                let inp = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                let cw = usage
                    .get("cache_creation_input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let cc = usage.get("cache_creation").and_then(|v| v.as_object());
                let cw5 = cc
                    .and_then(|m| m.get("ephemeral_5m_input_tokens"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let cw1h = cc
                    .and_then(|m| m.get("ephemeral_1h_input_tokens"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let cr = usage
                    .get("cache_read_input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let out = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                let think = usage
                    .get("output_tokens_details")
                    .and_then(|v| v.get("thinking_tokens"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                if let Some(&idx) = turn_id_to_idx.get(&mid) {
                    session.duplicate_api_lines_skipped += 1;
                    let prev = &mut session.turns[idx];
                    prev.uncached_input = prev.uncached_input.max(inp);
                    prev.cache_write = prev.cache_write.max(cw);
                    prev.cache_write_5m = prev.cache_write_5m.max(cw5);
                    prev.cache_write_1h = prev.cache_write_1h.max(cw1h);
                    prev.cache_read = prev.cache_read.max(cr);
                    prev.output = prev.output.max(out);
                    prev.reasoning = prev.reasoning.max(think);
                } else {
                    let idx = session.turns.len();
                    turn_id_to_idx.insert(mid, idx);
                    session.turns.push(TurnUsage {
                        model,
                        uncached_input: inp,
                        cache_write: cw,
                        cache_write_5m: cw5,
                        cache_write_1h: cw1h,
                        cache_read: cr,
                        output: out,
                        reasoning: think,
                    });
                    current_turn_idx = idx;
                }
            }

            if let Some(content) = msg.get("content").and_then(|v| v.as_array()) {
                for block in content {
                    if block.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                        let tname = block.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
                        let (cat, prog, ranged) = classify_tool(tname, block.get("input"));
                        let followed_search = cat.starts_with("Read")
                            && last_cat.as_deref().map(|c| c.starts_with("Search")).unwrap_or(false);
                        if let Some(tid) = block.get("id").and_then(|v| v.as_str()) {
                            tool_meta.insert(
                                tid.to_string(),
                                (current_turn_idx, cat.clone(), prog, ranged, followed_search),
                            );
                        }
                        last_cat = Some(cat);
                    }
                }
            }
        } else if rec_type == "user" {
            if let Some(content) = obj
                .get("message")
                .and_then(|v| v.get("content"))
                .and_then(|v| v.as_array())
            {
                for block in content {
                    if block.get("type").and_then(|v| v.as_str()) == Some("tool_result") {
                        let tid = block.get("tool_use_id").and_then(|v| v.as_str()).unwrap_or("");
                        let (t_idx, cat, prog, ranged, followed_search) = tool_meta
                            .get(tid)
                            .cloned()
                            .unwrap_or_else(|| (current_turn_idx, "Unknown".into(), String::new(), false, false));
                        let toks = block
                            .get("content")
                            .map(|c| count_tokens(&extract_text(c)))
                            .unwrap_or(0);
                        session.observations.push(ToolObservation {
                            turn_index: t_idx,
                            category: cat,
                            program: prog,
                            tokens: toks,
                            is_ranged_read: ranged,
                            followed_search,
                        });
                    }
                }
            }
        }
    }

    session
}

/// Parse an Antigravity `transcript.jsonl` (`PLANNER_RESPONSE` + sequential `GENERIC` tool outputs).
/// Note: in Antigravity transcripts, `input_tokens` (uncached new input) and `cache_read_tokens`
/// are DISJOINT, so total_input = input_tokens + cache_read_tokens.
pub fn parse_antigravity_str(raw: &str, id_seed: &str) -> SessionMetrics {
    let mut session = SessionMetrics {
        session_id: anonymize_id(id_seed),
        agent: "antigravity".into(),
        is_sidechain: false,
        mtime_secs: 0,
        turns: Vec::new(),
        observations: Vec::new(),
        duplicate_api_lines_skipped: 0,
    };

    let mut pending_tools: Vec<(usize, String, String, bool, bool)> = Vec::new();
    let mut pending_cursor = 0usize;
    let mut last_cat: Option<String> = None;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(obj) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        let step_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");

        if step_type == "PLANNER_RESPONSE" {
            let inp = obj.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
            let cr = obj.get("cache_read_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
            let out = obj.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
            let model = obj
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("antigravity")
                .to_string();
            let thinking_tok = obj
                .get("thinking")
                .and_then(|v| v.as_str())
                .map(count_tokens)
                .unwrap_or(0);

            let idx = session.turns.len();
            if inp > 0 || cr > 0 || out > 0 {
                session.turns.push(TurnUsage {
                    model,
                    uncached_input: inp,
                    cache_write: 0,
                    cache_write_5m: 0,
                    cache_write_1h: 0,
                    cache_read: cr,
                    output: out,
                    reasoning: thinking_tok,
                });
            }

            pending_tools.clear();
            pending_cursor = 0;
            if let Some(tcs) = obj.get("tool_calls").and_then(|v| v.as_array()) {
                for tc in tcs {
                    let tname = tc.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
                    let (cat, prog, ranged) = classify_tool(tname, tc.get("args"));
                    let followed_search = cat.starts_with("Read")
                        && last_cat.as_deref().map(|c| c.starts_with("Search")).unwrap_or(false);
                    pending_tools.push((idx, cat.clone(), prog, ranged, followed_search));
                    last_cat = Some(cat);
                }
            }
        } else if step_type == "GENERIC" && pending_cursor < pending_tools.len() {
            let (t_idx, cat, prog, ranged, followed_search) = pending_tools[pending_cursor].clone();
            pending_cursor += 1;
            let content = obj.get("content").map(extract_text).unwrap_or_default();
            session.observations.push(ToolObservation {
                turn_index: t_idx,
                category: cat,
                program: prog,
                tokens: count_tokens(&content),
                is_ranged_read: ranged,
                followed_search,
            });
        }
    }

    session
}

pub fn parse_codex_str(raw: &str, id_seed: &str) -> SessionMetrics {
    let mut session = SessionMetrics {
        session_id: anonymize_id(id_seed),
        agent: "codex".into(),
        is_sidechain: false,
        mtime_secs: 0,
        turns: Vec::new(),
        observations: Vec::new(),
        duplicate_api_lines_skipped: 0,
    };

    let mut prev_cum: Option<(u64, u64, u64, u64)> = None;
    let mut call_meta: HashMap<String, (usize, String, String, bool, bool)> = HashMap::new();
    let mut last_cat: Option<String> = None;
    let mut model = "codex".to_string();

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(obj) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        let rtype = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let payload = obj
            .get("payload")
            .filter(|v| v.is_object())
            .unwrap_or(&obj);
        let ptype = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");

        if rtype == "turn_context" || ptype == "turn_context" {
            if let Some(m) = payload.get("model").and_then(|v| v.as_str()) {
                model = m.to_string();
            }
        }

        if rtype == "token_count" || (rtype == "event_msg" && ptype == "token_count") {
            let info = payload.get("info").filter(|v| v.is_object()).unwrap_or(payload);
            let tot_u = info.get("total_token_usage").and_then(|v| v.as_object());
            let last_u = info.get("last_token_usage").and_then(|v| v.as_object());

            let (raw_in, cached_in, out, reas) = if let Some(tu) = tot_u {
                let cum_sig = (
                    tu.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    tu.get("cached_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    tu.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    tu.get("reasoning_output_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                );
                if Some(cum_sig) == prev_cum {
                    session.duplicate_api_lines_skipped += 1;
                    continue;
                }
                let res = if let Some(lu) = last_u {
                    (
                        lu.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                        lu.get("cached_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                        lu.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                        lu.get("reasoning_output_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    )
                } else if let Some(pc) = prev_cum {
                    (
                        cum_sig.0.saturating_sub(pc.0),
                        cum_sig.1.saturating_sub(pc.1),
                        cum_sig.2.saturating_sub(pc.2),
                        cum_sig.3.saturating_sub(pc.3),
                    )
                } else {
                    cum_sig
                };
                prev_cum = Some(cum_sig);
                res
            } else if let Some(lu) = last_u {
                (
                    lu.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    lu.get("cached_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    lu.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    lu.get("reasoning_output_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                )
            } else {
                continue;
            };

            let uncached = raw_in.saturating_sub(cached_in);
            session.turns.push(TurnUsage {
                model: model.clone(),
                uncached_input: uncached,
                cache_write: 0,
                cache_write_5m: 0,
                cache_write_1h: 0,
                cache_read: cached_in,
                output: out,
                reasoning: reas,
            });
        } else if rtype == "response_item" || matches!(ptype, "function_call" | "function_call_output") {
            let cur_idx = session.turns.len().saturating_sub(1);
            if matches!(ptype, "function_call" | "custom_tool_call") {
                let tname = payload.get("name").and_then(|v| v.as_str()).unwrap_or("shell");
                let parsed_args = match payload.get("arguments").or_else(|| payload.get("input")) {
                    Some(Value::String(s)) => {
                        serde_json::from_str::<Value>(s).unwrap_or_else(|_| serde_json::json!({"command": s}))
                    }
                    Some(v) => v.clone(),
                    None => Value::Null,
                };
                let (cat, prog, ranged) = classify_tool(tname, Some(&parsed_args));
                let followed_search = cat.starts_with("Read")
                    && last_cat.as_deref().map(|c| c.starts_with("Search")).unwrap_or(false);
                if let Some(cid) = payload
                    .get("call_id")
                    .or_else(|| payload.get("id"))
                    .and_then(|v| v.as_str())
                {
                    call_meta.insert(
                        cid.to_string(),
                        (cur_idx, cat.clone(), prog, ranged, followed_search),
                    );
                }
                last_cat = Some(cat);
            } else if matches!(ptype, "function_call_output" | "custom_tool_call_output") {
                let cid = payload
                    .get("call_id")
                    .or_else(|| payload.get("id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let (t_idx, cat, prog, ranged, followed_search) = call_meta
                    .get(cid)
                    .cloned()
                    .unwrap_or_else(|| (cur_idx, "Shell (other)".into(), String::new(), false, false));
                let out_text = payload.get("output").map(extract_text).unwrap_or_default();
                session.observations.push(ToolObservation {
                    turn_index: t_idx,
                    category: cat,
                    program: prog,
                    tokens: count_tokens(&out_text),
                    is_ranged_read: ranged,
                    followed_search,
                });
            }
        }
    }

    session
}

pub fn parse_gemini_str(raw: &str, id_seed: &str) -> SessionMetrics {
    let mut session = SessionMetrics {
        session_id: anonymize_id(id_seed),
        agent: "gemini".into(),
        is_sidechain: false,
        mtime_secs: 0,
        turns: Vec::new(),
        observations: Vec::new(),
        duplicate_api_lines_skipped: 0,
    };

    let Ok(data) = serde_json::from_str::<Value>(raw) else {
        return session;
    };
    let messages = data
        .get("messages")
        .and_then(|v| v.as_array())
        .or_else(|| data.as_array());
    let Some(msgs) = messages else {
        return session;
    };

    let mut last_cat: Option<String> = None;
    for msg in msgs {
        let Some(m) = msg.as_object() else {
            continue;
        };
        if let Some(um) = m
            .get("usageMetadata")
            .or_else(|| m.get("tokens"))
            .and_then(|v| v.as_object())
        {
            let prompt = um
                .get("promptTokenCount")
                .or_else(|| um.get("input"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let cached = um
                .get("cachedContentTokenCount")
                .or_else(|| um.get("cached"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let cand = um
                .get("candidatesTokenCount")
                .or_else(|| um.get("output"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let thoughts = um
                .get("thoughtsTokenCount")
                .or_else(|| um.get("thoughts"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let uncached = prompt.saturating_sub(cached);
            session.turns.push(TurnUsage {
                model: m
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("gemini")
                    .to_string(),
                uncached_input: uncached,
                cache_write: 0,
                cache_write_5m: 0,
                cache_write_1h: 0,
                cache_read: cached,
                output: cand + thoughts,
                reasoning: thoughts,
            });
        }

        let cur_idx = session.turns.len().saturating_sub(1);
        if let Some(tcs) = m.get("toolCalls").and_then(|v| v.as_array()) {
            for tc in tcs {
                let tname = tc.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
                let (cat, prog, ranged) = classify_tool(tname, tc.get("args"));
                let followed_search = cat.starts_with("Read")
                    && last_cat.as_deref().map(|c| c.starts_with("Search")).unwrap_or(false);
                last_cat = Some(cat.clone());
                if let Some(res) = tc.get("result").or_else(|| tc.get("response")) {
                    let text = extract_text(res);
                    if !text.is_empty() {
                        session.observations.push(ToolObservation {
                            turn_index: cur_idx,
                            category: cat,
                            program: prog,
                            tokens: count_tokens(&text),
                            is_ranged_read: ranged,
                            followed_search,
                        });
                    }
                }
            }
        }
    }

    session
}

pub fn detect_and_parse(path: &Path, agent: &str) -> Option<SessionMetrics> {
    if !path.is_file() {
        return None;
    }
    let id_seed = path.display().to_string();
    let is_sub = path.components().any(|c| c.as_os_str() == "subagents");
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let path_lower = id_seed.to_ascii_lowercase();

    // Fast path-based filtering BEFORE reading potentially large files from disk
    let resolved_agent: &str = if agent != "auto" {
        agent
    } else if ext == "json" {
        if (path_lower.contains(".gemini") && path_lower.contains("chats"))
            || name.contains("gemini")
            || name.contains("chat")
        {
            "gemini"
        } else {
            return None;
        }
    } else if ext == "jsonl" {
        if name == "transcript_full.jsonl" {
            return None;
        }
        if name == "transcript.jsonl" || path_lower.contains("antigravity") {
            "antigravity"
        } else if path_lower.contains(".codex") || name.starts_with("rollout-") {
            "codex"
        } else if path_lower.contains(".claude") {
            "claude"
        } else {
            "peek"
        }
    } else {
        return None;
    };

    let raw = fs::read_to_string(path).ok()?;
    let mtime_secs = fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut parsed = match resolved_agent {
        "claude" => Some(parse_claude_str(&raw, &id_seed, is_sub)),
        "antigravity" => Some(parse_antigravity_str(&raw, &id_seed)),
        "codex" => Some(parse_codex_str(&raw, &id_seed)),
        "gemini" => Some(parse_gemini_str(&raw, &id_seed)),
        _ => {
            let mut found = None;
            for line in raw.lines().take(5) {
                let t = line.trim();
                if t.is_empty() {
                    continue;
                }
                if t.contains("\"PLANNER_RESPONSE\"") || t.contains("\"USER_INPUT\"") {
                    found = Some(parse_antigravity_str(&raw, &id_seed));
                    break;
                }
                if t.contains("\"event_msg\"")
                    || t.contains("\"turn_context\"")
                    || t.contains("\"token_count\"")
                {
                    found = Some(parse_codex_str(&raw, &id_seed));
                    break;
                }
                found = Some(parse_claude_str(&raw, &id_seed, is_sub));
                break;
            }
            found
        }
    };
    if let Some(ref mut s) = parsed {
        s.mtime_secs = mtime_secs;
    }
    parsed
}

fn collect_transcript_files(root: &Path, known_agent: Option<&str>, out: &mut Vec<(PathBuf, String)>) {
    if root.is_file() {
        out.push((
            root.to_path_buf(),
            known_agent.unwrap_or("auto").to_string(),
        ));
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let mut sorted: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    sorted.sort_by_key(|e| e.file_name());

    for entry in sorted {
        let p = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if p.is_dir() {
            // Never recurse into code/build/scratch folders inside agent storage
            if matches!(
                name_str.as_ref(),
                "node_modules" | ".git" | "target" | ".venv" | "scratch" | "steps" | "tasks"
            ) {
                continue;
            }
            collect_transcript_files(&p, known_agent, out);
        } else if p.is_file() {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            let keep = match known_agent {
                Some("antigravity") => name_str == "transcript.jsonl",
                Some("claude") | Some("codex") => ext.eq_ignore_ascii_case("jsonl"),
                Some("gemini") => {
                    ext.eq_ignore_ascii_case("json")
                        && p.parent()
                            .and_then(|d| d.file_name())
                            .and_then(|n| n.to_str())
                            == Some("chats")
                }
                _ => {
                    name_str != "transcript_full.jsonl"
                        && (ext.eq_ignore_ascii_case("jsonl") || ext.eq_ignore_ascii_case("json"))
                }
            };
            if keep {
                out.push((p, known_agent.unwrap_or("auto").to_string()));
            }
        }
    }
}

/// Auto-discover all default transcript directories across Claude Code, Antigravity, Codex CLI, and Gemini CLI.
pub fn discover_default_roots(agent_filter: &str) -> Vec<(PathBuf, &'static str)> {
    let Some(home) = user_home() else {
        return Vec::new();
    };
    let mut roots = Vec::new();
    let allow = |a: &str| agent_filter == "auto" || agent_filter == a;

    if allow("claude") {
        let p = home.join(".claude").join("projects");
        if p.is_dir() {
            roots.push((p, "claude"));
        }
    }
    if allow("antigravity") {
        for sub in ["antigravity", "antigravity-cli", "antigravity-ide"] {
            let p = home.join(".gemini").join(sub).join("brain");
            if p.is_dir() {
                roots.push((p, "antigravity"));
            }
        }
    }
    if allow("codex") {
        let p = home.join(".codex").join("sessions");
        if p.is_dir() {
            roots.push((p, "codex"));
        }
    }
    if allow("gemini") {
        let p = home.join(".gemini").join("tmp");
        if p.is_dir() {
            roots.push((p, "gemini"));
        }
    }
    roots
}

#[derive(Debug, Default, Serialize)]
pub struct AgentBreakdown {
    pub sessions: usize,
    pub turns: usize,
    pub raw_tokens: u64,
    pub cache_read: u64,
    pub total_input: u64,
    pub cost_weighted: f64,
    pub estimated_usd: f64,
}

#[derive(Debug, Default, Serialize)]
pub struct DailyBucket {
    pub date: String,
    pub sessions: usize,
    pub turns: usize,
    pub raw_tokens: u64,
    pub cache_read: u64,
    pub total_input: u64,
    pub estimated_usd: f64,
}

#[derive(Debug, Default, Serialize)]
pub struct ModelBreakdown {
    pub turns: usize,
    pub raw_tokens: u64,
    pub cost_weighted: f64,
    pub estimated_usd: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdvisorTip {
    pub severity: String, // "high", "medium", "good"
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Default, Serialize)]
pub struct CorpusSummary {
    pub total_files: usize,
    pub parent_sessions: usize,
    pub subagent_sessions: usize,
    pub parent_turns: usize,
    pub subagent_turns: usize,
    pub duplicate_api_lines_skipped: usize,
    pub uncached_input: u64,
    pub cache_write: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
    pub cache_read: u64,
    pub output: u64,
    pub reasoning: u64,
    pub total_cost_weighted: f64,
    pub subagent_cost_weighted: f64,
    pub total_estimated_usd: f64,
    pub by_agent: BTreeMap<String, AgentBreakdown>,
    pub by_model: BTreeMap<String, ModelBreakdown>,
    pub by_day: BTreeMap<String, DailyBucket>,
    pub calls_by_cat: BTreeMap<String, usize>,
    pub first_tok_by_cat: BTreeMap<String, u64>,
    pub resent_tok_by_cat: BTreeMap<String, u64>,
    pub calls_by_prog: BTreeMap<String, usize>,
    pub first_tok_by_prog: BTreeMap<String, u64>,
    pub read_full_count: usize,
    pub read_ranged_count: usize,
    pub read_after_search_count: usize,
}

impl CorpusSummary {
    pub fn total_turns(&self) -> usize {
        self.parent_turns + self.subagent_turns
    }
    pub fn total_input(&self) -> u64 {
        self.uncached_input + self.cache_write + self.cache_read
    }
    pub fn total_raw_tokens(&self) -> u64 {
        self.total_input() + self.output
    }
    pub fn cache_hit_rate(&self) -> f64 {
        let tot = self.total_input();
        if tot == 0 {
            0.0
        } else {
            (self.cache_read as f64) / (tot as f64)
        }
    }
    pub fn amplification_factor(&self) -> f64 {
        let new_in = self.uncached_input + self.cache_write;
        if new_in == 0 {
            1.0
        } else {
            (self.total_input() as f64) / (new_in as f64)
        }
    }
}

pub fn summarize_corpus(sessions: &[SessionMetrics]) -> CorpusSummary {
    let mut s = CorpusSummary::default();
    for sess in sessions {
        if sess.turn_count() == 0 && sess.observations.is_empty() {
            continue;
        }
        s.total_files += 1;
        s.duplicate_api_lines_skipped += sess.duplicate_api_lines_skipped;
        let t_count = sess.turn_count();
        let cw = sess.cost_weighted();
        let usd = sess.estimated_usd();

        if sess.is_sidechain {
            s.subagent_sessions += 1;
            s.subagent_turns += t_count;
            s.subagent_cost_weighted += cw;
        } else {
            s.parent_sessions += 1;
            s.parent_turns += t_count;
        }

        s.uncached_input += sess.uncached_input();
        s.cache_write += sess.cache_write();
        s.cache_write_5m += sess.cache_write_5m();
        s.cache_write_1h += sess.cache_write_1h();
        s.cache_read += sess.cache_read();
        s.output += sess.output();
        s.reasoning += sess.reasoning();
        s.total_cost_weighted += cw;
        s.total_estimated_usd += usd;

        let ab = s.by_agent.entry(sess.agent.clone()).or_default();
        ab.sessions += 1;
        ab.turns += t_count;
        ab.raw_tokens += sess.total_input() + sess.output();
        ab.cache_read += sess.cache_read();
        ab.total_input += sess.total_input();
        ab.cost_weighted += cw;
        ab.estimated_usd += usd;

        if sess.mtime_secs > 0 {
            let date = unix_secs_to_date(sess.mtime_secs);
            let db = s.by_day.entry(date.clone()).or_insert_with(|| DailyBucket {
                date,
                ..Default::default()
            });
            db.sessions += 1;
            db.turns += t_count;
            db.raw_tokens += sess.total_input() + sess.output();
            db.cache_read += sess.cache_read();
            db.total_input += sess.total_input();
            db.estimated_usd += usd;
        }

        for turn in &sess.turns {
            let mb = s.by_model.entry(turn.model.clone()).or_default();
            mb.turns += 1;
            mb.raw_tokens += turn.total_input() + turn.output;
            mb.cost_weighted += turn.cost_weighted(&sess.agent);
            mb.estimated_usd += turn.estimated_usd(&sess.agent);
        }

        for obs in &sess.observations {
            let rem = t_count.saturating_sub(obs.turn_index).max(1) as u64;
            let resent = obs.tokens * rem;
            *s.calls_by_cat.entry(obs.category.clone()).or_insert(0) += 1;
            *s.first_tok_by_cat.entry(obs.category.clone()).or_insert(0) += obs.tokens;
            *s.resent_tok_by_cat.entry(obs.category.clone()).or_insert(0) += resent;
            if !obs.program.is_empty() {
                *s.calls_by_prog.entry(obs.program.clone()).or_insert(0) += 1;
                *s.first_tok_by_prog.entry(obs.program.clone()).or_insert(0) += obs.tokens;
            }

            if obs.category == "Read (tool)" {
                if obs.is_ranged_read {
                    s.read_ranged_count += 1;
                } else {
                    s.read_full_count += 1;
                }
                if obs.followed_search {
                    s.read_after_search_count += 1;
                }
            }
        }
    }
    s
}

pub fn load_sessions(path_opt: Option<&Path>, agent: &str) -> Vec<SessionMetrics> {
    let mut files: Vec<(PathBuf, String)> = Vec::new();
    if let Some(p) = path_opt {
        let known = if agent == "auto" { None } else { Some(agent) };
        collect_transcript_files(p, known, &mut files);
    } else {
        for (r, ag) in discover_default_roots(agent) {
            collect_transcript_files(&r, Some(ag), &mut files);
        }
    }
    files
        .iter()
        .filter_map(|(f, ag)| detect_and_parse(f, ag))
        .filter(|s| s.turn_count() > 0 || !s.observations.is_empty())
        .collect()
}

/// Load sessions filtered by optional age window (`since_days`) and optional cap (`max_files`).
/// Sorted newest-first so `--since 14d` or dashboard refreshes stay snappy even with thousands of sessions.
pub fn load_sessions_window(
    path_opt: Option<&Path>,
    agent: &str,
    since_days: Option<u64>,
    max_files: Option<usize>,
) -> Vec<SessionMetrics> {
    let mut files: Vec<(PathBuf, String)> = Vec::new();
    if let Some(p) = path_opt {
        let known = if agent == "auto" { None } else { Some(agent) };
        collect_transcript_files(p, known, &mut files);
    } else {
        for (r, ag) in discover_default_roots(agent) {
            collect_transcript_files(&r, Some(ag), &mut files);
        }
    }

    let cutoff = since_days.map(|days| {
        crate::telemetry::now_unix_secs().saturating_sub(days.saturating_mul(86_400))
    });

    let mut with_mtime: Vec<(u64, PathBuf, String)> = files
        .into_iter()
        .filter_map(|(p, ag)| {
            let mt = fs::metadata(&p)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            if let Some(c) = cutoff {
                if mt < c {
                    return None;
                }
            }
            Some((mt, p, ag))
        })
        .collect();

    with_mtime.sort_by_key(|a| std::cmp::Reverse(a.0));
    let iter = with_mtime.into_iter();
    let sliced: Vec<_> = match max_files {
        Some(lim) => iter.take(lim).collect(),
        None => iter.collect(),
    };

    sliced
        .into_iter()
        .filter_map(|(_, f, ag)| detect_and_parse(&f, &ag))
        .filter(|s| s.turn_count() > 0 || !s.observations.is_empty())
        .collect()
}

/// Find the single most recently modified transcript file across all detected agents (for `eai watch`).
pub fn newest_transcript_file(agent: &str) -> Option<(PathBuf, String, u64)> {
    let mut files: Vec<(PathBuf, String)> = Vec::new();
    for (r, ag) in discover_default_roots(agent) {
        collect_transcript_files(&r, Some(ag), &mut files);
    }
    files
        .into_iter()
        .filter_map(|(p, ag)| {
            let mt = fs::metadata(&p)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())?;
            Some((p, ag, mt))
        })
        .max_by_key(|x| x.2)
}

/// Compute concrete, data-driven optimization tips from session corpus + static prefix audit.
pub fn compute_advisor_tips(
    summary: &CorpusSummary,
    prefix_tokens: usize,
    recovery_rate_pct: f64,
) -> Vec<AdvisorTip> {
    let mut tips = Vec::new();
    let total_reads = summary.read_full_count + summary.read_ranged_count;
    if total_reads >= 5 {
        let full_pct = 100.0 * (summary.read_full_count as f64) / (total_reads as f64);
        if full_pct > 45.0 {
            tips.push(AdvisorTip {
                severity: "high".into(),
                title: format!("Whole-file reads are {:.0}% of all Read calls", full_pct),
                detail: "Use `eai outline <file>` before full reads or set `read_mode = \"outline-first\"` in `.eai/config.toml` to cut read tokens by ~40-50%.".into(),
            });
        } else {
            tips.push(AdvisorTip {
                severity: "good".into(),
                title: format!("Strong read discipline ({:.0}% ranged reads)", 100.0 - full_pct),
                detail: "Targeted ranged reads and symbol outlines are keeping context lean.".into(),
            });
        }
    }

    if prefix_tokens > 2500 {
        let cost_50 = ((prefix_tokens as f64) * 6.15) as usize;
        tips.push(AdvisorTip {
            severity: "high".into(),
            title: format!("High idle static prefix ({} tok/turn)", prefix_tokens),
            detail: format!(
                "Paid on every turn (~{} cost-weighted tok per 50-turn session). Run `eai audit . --include-user` to trim bloated skills or MCP configs.",
                cost_50
            ),
        });
    } else if prefix_tokens > 0 {
        tips.push(AdvisorTip {
            severity: "good".into(),
            title: format!("Lean static prefix ({} tok/turn)", prefix_tokens),
            detail: "Well below the 2,500 tok/turn ceiling — minimal per-turn tax.".into(),
        });
    }

    if summary.total_turns() >= 10 {
        let chr = summary.cache_hit_rate() * 100.0;
        if chr < 90.0 {
            tips.push(AdvisorTip {
                severity: "high".into(),
                title: format!("Prompt-cache hit rate is low ({:.1}%)", chr),
                detail: "Check for dynamic timestamps in system prompts, mid-session tool list mutations, or per-turn history pruning (see Exp 007).".into(),
            });
        } else {
            tips.push(AdvisorTip {
                severity: "good".into(),
                title: format!("Healthy prompt-cache hit rate ({:.1}%)", chr),
                detail: "Prefix stability is preserving 0.1x-0.25x cached input pricing.".into(),
            });
        }
    }

    if summary.total_cost_weighted > 0.0 {
        let sub_pct = 100.0 * summary.subagent_cost_weighted / summary.total_cost_weighted;
        if sub_pct > 25.0 {
            tips.push(AdvisorTip {
                severity: "medium".into(),
                title: format!("Sub-agents account for {:.1}% of total spend", sub_pct),
                detail: "Keep sub-agent return summaries terse; returned summaries are re-sent ~233x in parent orchestrator sessions.".into(),
            });
        }
    }

    if recovery_rate_pct > 5.0 {
        tips.push(AdvisorTip {
            severity: "medium".into(),
            title: format!("Elevated squeeze recovery rate ({:.1}%)", recovery_rate_pct),
            detail: "Agents are frequently opening `.eai/logs/`. Consider raising `--max-lines 200` for noisy commands.".into(),
        });
    }

    tips
}

/// Parse `--since 14d` or `--since 30` into days.
pub fn parse_since_days(spec: &str) -> Option<u64> {
    spec.trim().trim_end_matches(['d', 'D']).parse().ok()
}

pub fn load_recent_sessions(agent: &str, max_files: usize) -> Vec<SessionMetrics> {
    load_sessions_window(None, agent, None, Some(max_files))
}

fn fmt_u64(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

pub fn format_analyze_markdown(s: &CorpusSummary) -> String {
    let mut out = String::new();
    out.push_str("# efficient-ai (`eai`) Multi-Agent Session Report\n\n");
    out.push_str("## 1. Corpus Overview\n\n");
    out.push_str(&format!(
        "- **Transcripts analyzed:** {} ({} parent, {} sub-agent)\n",
        fmt_u64(s.total_files as u64),
        fmt_u64(s.parent_sessions as u64),
        fmt_u64(s.subagent_sessions as u64)
    ));
    out.push_str(&format!(
        "- **Deduplicated API turns:** {} ({} duplicate streaming lines merged)\n",
        fmt_u64(s.total_turns() as u64),
        fmt_u64(s.duplicate_api_lines_skipped as u64)
    ));
    out.push_str(&format!(
        "- **Prompt-cache hit rate:** **{:.2}%**\n",
        100.0 * s.cache_hit_rate()
    ));
    out.push_str(&format!(
        "- **History amplification factor:** **{:.1}×**\n",
        s.amplification_factor()
    ));
    out.push_str(&format!(
        "- **Total raw tokens:** {} | **Cost-weighted tokens:** **{}** (~**${:.2} USD** estimated list price)\n\n",
        fmt_u64(s.total_raw_tokens()),
        fmt_u64(s.total_cost_weighted.round() as u64),
        s.total_estimated_usd
    ));

    if !s.by_agent.is_empty() {
        out.push_str("## 2. Breakdown by Agent (Per-Agent Pricing Weights)\n\n");
        out.push_str("| Agent | Sessions | Turns | Raw Tokens | Cache Hit % | Cost-Weighted Tok | Est. USD |\n");
        out.push_str("|---|---:|---:|---:|---:|---:|---:|\n");
        for (agent, ab) in &s.by_agent {
            let chr = if ab.total_input > 0 {
                100.0 * (ab.cache_read as f64) / (ab.total_input as f64)
            } else {
                0.0
            };
            out.push_str(&format!(
                "| **{}** | {} | {} | {} | {:.1}% | {} | **${:.2}** |\n",
                agent,
                fmt_u64(ab.sessions as u64),
                fmt_u64(ab.turns as u64),
                fmt_u64(ab.raw_tokens),
                chr,
                fmt_u64(ab.cost_weighted.round() as u64),
                ab.estimated_usd
            ));
        }
        out.push('\n');
    }

    if !s.first_tok_by_cat.is_empty() {
        out.push_str("## 3. Tool-Output Attribution (First-Occurrence vs. Re-sent Exposure)\n\n");
        out.push_str("| Category | Calls | First-Occ Tok | Share (1×) | Re-sent Exposure | Share (Re-sent) | Avg Amp |\n");
        out.push_str("|---|---:|---:|---:|---:|---:|---:|\n");

        let mut cats: Vec<(&String, &u64)> = s.first_tok_by_cat.iter().collect();
        cats.sort_by(|a, b| b.1.cmp(a.1));
        let tot_first: u64 = s.first_tok_by_cat.values().sum::<u64>().max(1);
        let tot_resent: u64 = s.resent_tok_by_cat.values().sum::<u64>().max(1);

        for (cat, &f_tok) in cats.into_iter().take(12) {
            let calls = *s.calls_by_cat.get(cat).unwrap_or(&0);
            let r_tok = *s.resent_tok_by_cat.get(cat).unwrap_or(&0);
            let amp = (r_tok as f64) / (f_tok.max(1) as f64);
            out.push_str(&format!(
                "| {} | {} | {} | {:.1}% | {} | **{:.1}%** | {:.1}× |\n",
                cat,
                fmt_u64(calls as u64),
                fmt_u64(f_tok),
                100.0 * (f_tok as f64) / (tot_first as f64),
                fmt_u64(r_tok),
                100.0 * (r_tok as f64) / (tot_resent as f64),
                amp
            ));
        }
        out.push('\n');
    }

    let total_reads = s.read_full_count + s.read_ranged_count;
    if total_reads > 0 {
        let full_pct = 100.0 * (s.read_full_count as f64) / (total_reads as f64);
        out.push_str("## 4. Read Discipline\n\n");
        out.push_str(&format!(
            "- **Read tool calls:** {} ({} whole-file [{:.1}%], {} ranged)\n",
            fmt_u64(total_reads as u64),
            fmt_u64(s.read_full_count as u64),
            full_pct,
            fmt_u64(s.read_ranged_count as u64)
        ));
        out.push_str(&format!(
            "- **Search-then-read pairs:** {}\n",
            fmt_u64(s.read_after_search_count as u64)
        ));
    }

    out
}

pub fn cmd_analyze(args: &[String]) -> i32 {
    let mut path_arg: Option<PathBuf> = None;
    let mut agent = "auto".to_string();
    let mut since_days: Option<u64> = None;
    let mut json_out = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--agent" if i + 1 < args.len() => {
                agent = args[i + 1].clone();
                i += 1;
            }
            "--since" if i + 1 < args.len() => {
                since_days = parse_since_days(&args[i + 1]);
                i += 1;
            }
            "--json" => json_out = true,
            "-h" | "--help" => {
                println!(
                    "Usage: eai analyze [path] [--agent auto|claude|antigravity|codex|gemini] [--since 14d] [--json]\n\
                     Auto-discovers Claude Code, Antigravity, Codex CLI, and Gemini CLI transcripts when [path] is omitted."
                );
                return 0;
            }
            other if !other.starts_with('-') => path_arg = Some(PathBuf::from(other)),
            _ => {}
        }
        i += 1;
    }

    if let Some(ref p) = path_arg {
        if !p.exists() {
            eprintln!("Error: path does not exist: {}", p.display());
            return 1;
        }
    }

    let sessions = load_sessions_window(path_arg.as_deref(), &agent, since_days, None);
    let summary = summarize_corpus(&sessions);

    if json_out {
        println!("{}", serde_json::to_string_pretty(&summary).unwrap_or_default());
    } else if summary.total_files == 0 {
        println!("No transcripts found for agent filter '{}'. Pass a path explicitly or start an agent session first.", agent);
    } else {
        print!("{}", format_analyze_markdown(&summary));
    }
    0
}

// =========================================================================
// Static-Prefix Auditor (`eai audit`)
// =========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct PrefixItem {
    pub category: String,
    pub label: String,
    pub loaded_when: String,
    pub est_tokens: usize,
    pub note: String,
}

fn extract_frontmatter(txt: &str) -> &str {
    if let Some(rest) = txt.strip_prefix("---\n").or_else(|| txt.strip_prefix("---\r\n")) {
        if let Some(end) = rest.find("\n---") {
            return &rest[..end];
        }
    }
    let end = txt.len().min(300);
    &txt[..end]
}

fn collect_skill_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut sorted: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    sorted.sort_by_key(|e| e.file_name());
    for entry in sorted {
        let p = entry.path();
        if p.is_dir() {
            collect_skill_files(&p, out);
        } else if p.file_name().and_then(|n| n.to_str()) == Some("SKILL.md") {
            out.push(p);
        }
    }
}

pub fn audit_directory(root: &Path, include_user: bool) -> Vec<PrefixItem> {
    let mut items = Vec::new();

    for fname in [
        "AGENTS.md",
        "CLAUDE.md",
        "GEMINI.md",
        ".cursorrules",
        ".github/copilot-instructions.md",
    ] {
        let p = root.join(fname);
        if p.is_file() {
            if let Ok(txt) = fs::read_to_string(&p) {
                let tok = est_tokens_from_chars(txt.len());
                let note = if txt.contains("@AGENTS.md") || txt.contains("@prompts/AGENTS.md") {
                    "Thin @import shim"
                } else {
                    "Paid on every turn"
                };
                items.push(PrefixItem {
                    category: "Instruction file".into(),
                    label: fname.into(),
                    loaded_when: "always (every turn)".into(),
                    est_tokens: tok,
                    note: note.into(),
                });
            }
        }
    }

    // Cursor rules (.cursor/rules/*.mdc)
    let cursor_rules = root.join(".cursor").join("rules");
    if cursor_rules.is_dir() {
        if let Ok(entries) = fs::read_dir(&cursor_rules) {
            for e in entries.filter_map(|x| x.ok()) {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) == Some("mdc") {
                    if let Ok(txt) = fs::read_to_string(&p) {
                        items.push(PrefixItem {
                            category: "Cursor rule".into(),
                            label: format!(".cursor/rules/{}", e.file_name().to_string_lossy()),
                            loaded_when: "always / glob".into(),
                            est_tokens: est_tokens_from_chars(txt.len()),
                            note: "Cursor rule file".into(),
                        });
                    }
                }
            }
        }
    }

    // Project skills
    for sdir_rel in [".claude/skills", ".gemini/skills", ".agents/skills", "skills"] {
        let sdir = root.join(sdir_rel);
        if !sdir.is_dir() {
            continue;
        }
        let mut smds = Vec::new();
        collect_skill_files(&sdir, &mut smds);
        for smd in smds {
            if let Ok(txt) = fs::read_to_string(&smd) {
                let fm = extract_frontmatter(&txt);
                let fm_tok = est_tokens_from_chars(fm.len());
                let body_tok = est_tokens_from_chars(txt.len());
                let skill_name = smd
                    .parent()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "skill".into());
                items.push(PrefixItem {
                    category: "Skill (idle metadata)".into(),
                    label: format!("{}/{}/SKILL.md", sdir_rel, skill_name),
                    loaded_when: "always (every turn)".into(),
                    est_tokens: fm_tok,
                    note: format!("Full body if triggered: {} tok", body_tok),
                });
            }
        }
    }

    // Project MCP / settings configs
    for cfg_rel in [
        ".mcp.json",
        "mcp_config.json",
        ".claude/settings.json",
        ".gemini/settings.json",
    ] {
        let p = root.join(cfg_rel);
        if p.is_file() {
            if let Ok(txt) = fs::read_to_string(&p) {
                let tok = est_tokens_from_chars(txt.len());
                let note = match serde_json::from_str::<Value>(&txt)
                    .ok()
                    .and_then(|v| serde_json::to_string(&v).ok())
                {
                    Some(min) => {
                        let min_tok = est_tokens_from_chars(min.len());
                        if tok > min_tok + 10 {
                            format!("Minifying saves {} tok", tok - min_tok)
                        } else {
                            "Compact".into()
                        }
                    }
                    None => "Config file".into(),
                };
                items.push(PrefixItem {
                    category: "Config / MCP".into(),
                    label: cfg_rel.into(),
                    loaded_when: "config / startup".into(),
                    est_tokens: tok,
                    note,
                });
            }
        }
    }

    if include_user {
        if let Some(home) = user_home() {
            for rel in [".claude/CLAUDE.md", ".gemini/GEMINI.md", ".codex/AGENTS.md"] {
                let p = home.join(rel);
                if p.is_file() {
                    if let Ok(txt) = fs::read_to_string(&p) {
                        items.push(PrefixItem {
                            category: "User global instruction".into(),
                            label: format!("~/{}", rel),
                            loaded_when: "always (every turn)".into(),
                            est_tokens: est_tokens_from_chars(txt.len()),
                            note: "Global prefix".into(),
                        });
                    }
                }
            }
            for sroot_rel in [
                ".claude/skills",
                ".gemini/skills",
                ".gemini/config/skills",
            ] {
                let sdir = home.join(sroot_rel);
                if !sdir.is_dir() {
                    continue;
                }
                let mut smds = Vec::new();
                collect_skill_files(&sdir, &mut smds);
                if !smds.is_empty() {
                    let mut tot_fm = 0usize;
                    let mut tot_body = 0usize;
                    for smd in &smds {
                        if let Ok(txt) = fs::read_to_string(smd) {
                            tot_fm += est_tokens_from_chars(extract_frontmatter(&txt).len());
                            tot_body += est_tokens_from_chars(txt.len());
                        }
                    }
                    items.push(PrefixItem {
                        category: "User global skills (idle)".into(),
                        label: format!("~/{} ({} skills)", sroot_rel, smds.len()),
                        loaded_when: "always (every turn)".into(),
                        est_tokens: tot_fm,
                        note: format!(
                            "Avg {} tok/skill idle; {} tok total bodies",
                            tot_fm / smds.len(),
                            tot_body
                        ),
                    });
                }
            }
        }
    }

    items
}

pub fn always_prefix_tokens(items: &[PrefixItem]) -> usize {
    items
        .iter()
        .filter(|i| i.loaded_when.starts_with("always"))
        .map(|i| i.est_tokens)
        .sum()
}

pub fn cmd_audit(args: &[String]) -> i32 {
    let mut root = PathBuf::from(".");
    let mut include_user = false;
    let mut json_out = false;

    for a in args {
        match a.as_str() {
            "--include-user" => include_user = true,
            "--json" => json_out = true,
            "-h" | "--help" => {
                println!("Usage: eai audit [path] [--include-user] [--json]");
                return 0;
            }
            other if !other.starts_with('-') => root = PathBuf::from(other),
            _ => {}
        }
    }

    let items = audit_directory(&root, include_user);
    if json_out {
        println!("{}", serde_json::to_string_pretty(&items).unwrap_or_default());
        return 0;
    }

    if items.is_empty() {
        println!("No static-prefix files (`AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, skills, or `.mcp.json`) found.");
        return 0;
    }

    let always_tok = always_prefix_tokens(&items);
    println!("# efficient-ai Static-Prefix Audit\n");
    println!("| Category | Source | Loaded | Est. Tokens | Notes |");
    println!("|---|---|---|---:|---|");
    for it in &items {
        println!(
            "| {} | `{}` | {} | {} | {} |",
            it.category, it.label, it.loaded_when, it.est_tokens, it.note
        );
    }
    let raw_50 = always_tok * 50;
    let cost_50 = ((always_tok as f64) * (1.25 + 49.0 * 0.10)) as usize;
    println!("\n## Session Tax Summary\n");
    println!("- **Idle static prefix:** **{} tokens / turn**", always_tok);
    println!("- **50-turn raw context footprint:** **{} tokens**", raw_50);
    println!(
        "- **50-turn billed cost (5m caching):** **{} input-token equivalents** ({} if cache busted every turn)",
        cost_50, raw_50
    );
    0
}

// =========================================================================
// Sanitize & Selftest
// =========================================================================

pub fn cmd_sanitize(args: &[String]) -> i32 {
    let mut path_arg: Option<PathBuf> = None;
    let mut agent = "auto".to_string();
    let mut out_path: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--agent" if i + 1 < args.len() => {
                agent = args[i + 1].clone();
                i += 1;
            }
            "--out" if i + 1 < args.len() => {
                out_path = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            other if !other.starts_with('-') => path_arg = Some(PathBuf::from(other)),
            _ => {}
        }
        i += 1;
    }

    let Some(out_file) = out_path else {
        eprintln!("Usage: eai sanitize [path] [--agent auto] --out <file.json>");
        return 2;
    };

    let sessions = load_sessions(path_arg.as_deref(), &agent);
    let records: Vec<Value> = sessions
        .iter()
        .map(|s| {
            let mut cat_tokens: BTreeMap<String, u64> = BTreeMap::new();
            for obs in &s.observations {
                *cat_tokens.entry(obs.category.clone()).or_insert(0) += obs.tokens;
            }
            serde_json::json!({
                "session_id": s.session_id,
                "agent": s.agent,
                "is_sidechain": s.is_sidechain,
                "turn_count": s.turn_count(),
                "uncached_input": s.uncached_input(),
                "cache_write": s.cache_write(),
                "cache_write_5m": s.cache_write_5m(),
                "cache_write_1h": s.cache_write_1h(),
                "cache_read": s.cache_read(),
                "output": s.output(),
                "reasoning": s.reasoning(),
                "cache_hit_rate": (s.cache_hit_rate() * 10000.0).round() / 10000.0,
                "cost_weighted_tokens": (s.cost_weighted() * 10.0).round() / 10.0,
                "estimated_usd": (s.estimated_usd() * 10000.0).round() / 10000.0,
                "tool_tokens_by_category": cat_tokens,
            })
        })
        .collect();

    if let Some(parent) = out_file.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(e) = fs::write(
        &out_file,
        format!("{}\n", serde_json::to_string_pretty(&records).unwrap_or_default()),
    ) {
        eprintln!("Error writing {}: {}", out_file.display(), e);
        return 1;
    }
    println!(
        "Wrote {} sanitized session records to {}",
        records.len(),
        out_file.display()
    );
    0
}

pub fn run_embedded_selftest() -> Result<(), String> {
    // 1. Claude streaming deduplication + 5m/1h cache split
    let claude_fx = r#"
{"type":"assistant","message":{"id":"m1","model":"claude-sonnet-5","content":[{"type":"thinking","thinking":"..."}],"usage":{"input_tokens":10,"cache_creation_input_tokens":2000,"cache_creation":{"ephemeral_5m_input_tokens":2000,"ephemeral_1h_input_tokens":0},"cache_read_input_tokens":15000,"output_tokens":3}}}
{"type":"assistant","message":{"id":"m1","model":"claude-sonnet-5","content":[{"type":"tool_use","id":"t1","name":"Grep","input":{"pattern":"foo"}}],"usage":{"input_tokens":10,"cache_creation_input_tokens":2000,"cache_creation":{"ephemeral_5m_input_tokens":2000,"ephemeral_1h_input_tokens":0},"cache_read_input_tokens":15000,"output_tokens":120}}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"a.py:1:def foo():"}]}}
{"type":"assistant","message":{"id":"m2","model":"claude-sonnet-5","content":[{"type":"tool_use","id":"t2","name":"Read","input":{"file_path":"a.py"}}],"usage":{"input_tokens":5,"cache_creation_input_tokens":300,"cache_creation":{"ephemeral_5m_input_tokens":0,"ephemeral_1h_input_tokens":300},"cache_read_input_tokens":17000,"output_tokens":80}}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":"def foo():\n    return 1\n"}]}}
"#;
    let c = parse_claude_str(claude_fx, "synth-claude", false);
    if c.turn_count() != 2 || c.duplicate_api_lines_skipped != 1 || c.turns[0].output != 120 {
        return Err(format!("Claude selftest mismatch: {:?}", c.turns));
    }
    if c.observations.len() != 2 || !c.observations[1].followed_search {
        return Err("Claude observation followed_search mismatch".into());
    }

    // 2. Antigravity disjoint input_tokens + cache_read_tokens + 1-to-1 GENERIC tool output
    let agy_fx = r###"
{"step_index":0,"source":"USER_EXPLICIT","type":"USER_INPUT","content":"Review repo"}
{"step_index":1,"source":"MODEL","type":"PLANNER_RESPONSE","model":"gemini-pro","input_tokens":14000,"cache_read_tokens":0,"output_tokens":400,"tool_calls":[{"name":"run_command","args":{"CommandLine":"git status"}},{"name":"read_file","args":{"Path":"README.md","StartLine":1,"EndLine":50}}]}
{"step_index":2,"source":"MODEL","type":"GENERIC","content":"## main"}
{"step_index":3,"source":"MODEL","type":"GENERIC","content":"# README"}
{"step_index":4,"source":"MODEL","type":"PLANNER_RESPONSE","model":"gemini-pro","input_tokens":2000,"cache_read_tokens":14000,"output_tokens":200}
"###;
    let a = parse_antigravity_str(agy_fx, "synth-agy");
    if a.turn_count() != 2 || a.uncached_input() != 16000 || a.cache_read() != 14000 {
        return Err(format!("Antigravity selftest mismatch: {:?}", a.turns));
    }
    if a.observations.len() != 2 || !a.observations[1].is_ranged_read {
        return Err("Antigravity observation mismatch".into());
    }

    // 3. Codex subset caching + duplicate token_count skip
    let codex_fx = r#"
{"type":"turn_context","payload":{"model":"gpt-5-codex"}}
{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10000,"cached_input_tokens":8000,"output_tokens":250,"reasoning_output_tokens":100},"last_token_usage":{"input_tokens":10000,"cached_input_tokens":8000,"output_tokens":250,"reasoning_output_tokens":100}}}}
{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10000,"cached_input_tokens":8000,"output_tokens":250,"reasoning_output_tokens":100},"last_token_usage":{"input_tokens":10000,"cached_input_tokens":8000,"output_tokens":250,"reasoning_output_tokens":100}}}}
"#;
    let x = parse_codex_str(codex_fx, "synth-codex");
    if x.turn_count() != 1 || x.duplicate_api_lines_skipped != 1 || x.uncached_input() != 2000 || x.cache_read() != 8000 {
        return Err(format!("Codex selftest mismatch: {:?}", x.turns));
    }

    // 4. Gemini subset caching + thoughts inclusion
    let gemini_fx = r#"{"messages":[{"type":"gemini","model":"gemini-2.5-pro","usageMetadata":{"promptTokenCount":12000,"cachedContentTokenCount":9000,"candidatesTokenCount":150,"thoughtsTokenCount":60},"toolCalls":[{"name":"read_file","args":{"path":"a.py"},"result":"print(1)"}]}]}"#;
    let g = parse_gemini_str(gemini_fx, "synth-gemini");
    if g.turn_count() != 1 || g.uncached_input() != 3000 || g.cache_read() != 9000 || g.output() != 210 {
        return Err(format!("Gemini selftest mismatch: {:?}", g.turns));
    }

    // 5. Mixed-agent corpus sums per-session cost weights cleanly
    let summary = summarize_corpus(&[c, a, x, g]);
    if summary.by_agent.len() != 4 || summary.total_estimated_usd <= 0.0 {
        return Err("Mixed-agent corpus summary mismatch".into());
    }

    Ok(())
}

pub fn cmd_selftest() -> i32 {
    match run_embedded_selftest() {
        Ok(()) => {
            println!("eai selftest: ALL 4 AGENT FIXTURES PASSED (Claude Code, Antigravity, Codex CLI, Gemini CLI)");
            0
        }
        Err(msg) => {
            eprintln!("eai selftest FAILED: {}", msg);
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_four_agent_fixtures() {
        run_embedded_selftest().expect("all 4 agent fixtures must pass");
    }
}
