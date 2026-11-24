//! Ultra-lean stdio MCP server (`eai mcp`) for Cursor, Windsurf, Cline, Roo Code,
//! VS Code Copilot, OpenCode, Claude Desktop, and Antigravity.
//!
//! Strictly honors H5 (Static-Prefix Diet): exposes 4 surgical tools with <280 idle tokens total.

use crate::telemetry::{
    est_tokens_from_chars, read_baseline, read_events, record_outline, record_squeeze,
    summarize_events,
};
use serde_json::{json, Value};
use std::env;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

/// Compact tool definitions (<280 tokens total on o200k/4-char calibration).
pub fn tool_definitions() -> Value {
    json!([
        {
            "name": "outline_file",
            "description": "Outline file symbols [Lstart-Lend] or read one symbol/line range.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "symbol": { "type": "string" },
                    "lines": { "type": "string" },
                    "budget": { "type": "integer" }
                },
                "required": ["path"]
            }
        },
        {
            "name": "outline_search",
            "description": "Search code and group hits by enclosing symbol.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pattern": { "type": "string" },
                    "path": { "type": "string" },
                    "expand": { "type": "boolean" }
                },
                "required": ["pattern"]
            }
        },
        {
            "name": "squeeze_run",
            "description": "Run command with lossless compression, spill log & loop guard.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "command": { "type": "string" },
                    "cwd": { "type": "string" },
                    "max_lines": { "type": "integer" }
                },
                "required": ["command"]
            }
        },
        {
            "name": "eai_stats",
            "description": "Show project token savings, recovery rate & baseline.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": { "type": "string" }
                }
            }
        }
    ])
}

fn resolve_path(p: &str, base: &Path) -> PathBuf {
    let pb = PathBuf::from(p);
    if pb.is_absolute() { pb } else { base.join(pb) }
}

fn call_tool(name: &str, args: &Value, cwd: &Path) -> Result<String, String> {
    match name {
        "outline_file" => {
            let path_s = args
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required 'path'".to_string())?;
            let target = resolve_path(path_s, cwd);
            if !target.exists() {
                return Err(format!("path not found: {}", target.display()));
            }
            if let Some(rng) = args.get("lines").and_then(|v| v.as_str()) {
                let (s, e) = outline::parse_range(rng)
                    .ok_or_else(|| format!("invalid lines range '{}'", rng))?;
                let (out, m) = outline::format_line_range(&target, s, e)
                    .map_err(|err| err.to_string())?;
                record_outline(cwd, m.action, m.raw_chars, m.output_chars);
                return Ok(out);
            }
            if let Some(sym) = args.get("symbol").and_then(|v| v.as_str()) {
                let (out, m) = outline::format_symbol(&target, sym)
                    .map_err(|err| err.to_string())?;
                record_outline(cwd, m.action, m.raw_chars, m.output_chars);
                return Ok(out);
            }
            let max_chars = args
                .get("budget")
                .and_then(|v| v.as_u64())
                .map(|b| (b as usize).saturating_mul(4));
            let (out, m) = outline::format_file_outline_budgeted(&target, max_chars)
                .map_err(|err| err.to_string())?;
            record_outline(cwd, m.action, m.raw_chars, m.output_chars);
            Ok(out)
        }
        "outline_search" => {
            let pattern = args
                .get("pattern")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required 'pattern'".to_string())?;
            let root = args
                .get("path")
                .and_then(|v| v.as_str())
                .map(|p| resolve_path(p, cwd))
                .unwrap_or_else(|| cwd.to_path_buf());
            let expand = args.get("expand").and_then(|v| v.as_bool()).unwrap_or(false);
            let (out, m) = outline::format_search(pattern, &root, expand, false);
            record_outline(cwd, m.action, m.raw_chars, m.output_chars);
            Ok(out)
        }
        "squeeze_run" => {
            let cmd = args
                .get("command")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing required 'command'".to_string())?;
            let run_cwd = args
                .get("cwd")
                .and_then(|v| v.as_str())
                .map(|p| resolve_path(p, cwd))
                .unwrap_or_else(|| cwd.to_path_buf());
            let max_lines = args
                .get("max_lines")
                .and_then(|v| v.as_u64())
                .unwrap_or(150) as usize;
            let (code, rendered, m) = squeeze::run_shell_capture(cmd, &run_cwd, max_lines);
            record_squeeze(
                &run_cwd,
                &m.command,
                m.raw_chars,
                m.kept_chars,
                m.elided_lines,
                m.log_path.is_some(),
                m.loop_detected,
            );
            Ok(format!("[exit {}]\n{}", code, rendered))
        }
        "eai_stats" => {
            let proj = args
                .get("project")
                .and_then(|v| v.as_str())
                .map(|p| resolve_path(p, cwd))
                .unwrap_or_else(|| cwd.to_path_buf());
            let events = read_events(&proj.join(".eai").join("metrics.jsonl"));
            let summary = summarize_events(&proj.display().to_string(), &events);
            let baseline = read_baseline(&proj);
            let payload = json!({ "stats": summary, "baseline": baseline });
            Ok(serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into()))
        }
        other => Err(format!("unknown tool '{}'", other)),
    }
}

/// Process a single MCP JSON-RPC 2.0 request object and return an optional response.
pub fn handle_rpc(req: &Value, cwd: &Path) -> Option<Value> {
    let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");
    let id = req.get("id").cloned();

    // JSON-RPC notifications have no `id` and expect no reply
    if id.is_none() || method.starts_with("notifications/") {
        return None;
    }
    let id_val = id.unwrap();

    match method {
        "initialize" => {
            let proto = req
                .get("params")
                .and_then(|p| p.get("protocolVersion"))
                .and_then(|v| v.as_str())
                .unwrap_or("2024-11-05");
            Some(json!({
                "jsonrpc": "2.0",
                "id": id_val,
                "result": {
                    "protocolVersion": proto,
                    "capabilities": { "tools": {} },
                    "serverInfo": {
                        "name": "efficient-ai",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }
            }))
        }
        "ping" => Some(json!({ "jsonrpc": "2.0", "id": id_val, "result": {} })),
        "tools/list" => Some(json!({
            "jsonrpc": "2.0",
            "id": id_val,
            "result": { "tools": tool_definitions() }
        })),
        "tools/call" => {
            let params = req.get("params").cloned().unwrap_or(Value::Null);
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            match call_tool(name, &args, cwd) {
                Ok(text) => Some(json!({
                    "jsonrpc": "2.0",
                    "id": id_val,
                    "result": {
                        "content": [{ "type": "text", "text": text }],
                        "isError": false
                    }
                })),
                Err(msg) => Some(json!({
                    "jsonrpc": "2.0",
                    "id": id_val,
                    "result": {
                        "content": [{ "type": "text", "text": msg }],
                        "isError": true
                    }
                })),
            }
        }
        _ => Some(json!({
            "jsonrpc": "2.0",
            "id": id_val,
            "error": { "code": -32601, "message": format!("Method not found: {}", method) }
        })),
    }
}

/// Run the stdio JSON-RPC MCP server loop.
pub fn cmd_mcp(args: &[String]) -> i32 {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        let schema_str = serde_json::to_string(&tool_definitions()).unwrap_or_default();
        println!(
            "eai mcp — Ultra-lean stdio MCP server (~{} idle tokens for all 4 tools)\n\
             Tools: outline_file, outline_search, squeeze_run, eai_stats\n\
             Connect via MCP config: {{\"command\": \"eai\", \"args\": [\"mcp\"]}}",
            est_tokens_from_chars(schema_str.len())
        );
        return 0;
    }

    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let Ok(raw) = line else { break };
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(req) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        if let Some(resp) = handle_rpc(&req, &cwd) {
            if let Ok(s) = serde_json::to_string(&resp) {
                let _ = writeln!(out, "{}", s);
                let _ = out.flush();
            }
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_schema_stays_under_280_idle_tokens() {
        let compact = serde_json::to_string(&tool_definitions()).unwrap();
        let est_tok = est_tokens_from_chars(compact.len());
        assert!(
            est_tok <= 280,
            "MCP idle schema exceeded H5 budget: {} tokens (>280)",
            est_tok
        );
    }

    #[test]
    fn test_mcp_initialize_list_and_call() {
        let cwd = env::current_dir().unwrap();
        let init = handle_rpc(
            &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}),
            &cwd,
        )
        .unwrap();
        assert_eq!(init["result"]["serverInfo"]["name"], "efficient-ai");

        let list = handle_rpc(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}), &cwd).unwrap();
        assert_eq!(list["result"]["tools"].as_array().unwrap().len(), 4);

        let stats = handle_rpc(
            &json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"eai_stats","arguments":{}}}),
            &cwd,
        )
        .unwrap();
        assert_eq!(stats["result"]["isError"], false);
    }
}
