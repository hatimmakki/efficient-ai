//! Sub-10ms native Rust hook router for Claude Code, Antigravity, and Gemini CLI.
//!
//! Invoked via:
//!   eai hook claude
//!   eai hook antigravity
//!   eai hook gemini

use crate::telemetry::{record_hook_rewrite, record_recovery, record_squeeze};
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// Quote an executable or file path safely for shell invocation across Unix & Windows.
fn quote_for_shell(s: &str) -> String {
    if !s.contains(' ') && !s.contains('\t') && !s.contains('"') && !s.contains('\'') {
        return s.to_string();
    }
    format!("\"{}\"", s.replace('"', "\\\""))
}

/// Minimal quote-aware shell splitter for single commands (handles `"path with spaces"` and `'...'`).
fn split_shell_tokens(cmd: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut chars = cmd.chars().peekable();

    while let Some(c) = chars.next() {
        if in_single {
            if c == '\'' {
                in_single = false;
            } else {
                cur.push(c);
            }
        } else if in_double {
            if c == '"' {
                in_double = false;
            } else if c == '\\' && matches!(chars.peek(), Some('"' | '\\')) {
                if let Some(next) = chars.next() {
                    cur.push(next);
                }
            } else {
                cur.push(c);
            }
        } else {
            match c {
                '\'' => in_single = true,
                '"' => in_double = true,
                c if c.is_whitespace() => {
                    if !cur.is_empty() {
                        tokens.push(std::mem::take(&mut cur));
                    }
                }
                _ => cur.push(c),
            }
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    tokens
}

fn count_lines(path: &Path) -> usize {
    fs::read_to_string(path)
        .map(|s| s.lines().count())
        .unwrap_or(0)
}

fn is_eai_log_path(path_str: &str) -> bool {
    let norm = path_str.replace('\\', "/");
    norm.contains(".eai/logs/") || norm.ends_with(".eai/logs/last.log")
}

/// Rewrite a shell command using the exact resolved `eai_exe` binary path (so it NEVER fails
/// with `command not found` even when `eai` is not on the subshell's PATH).
pub fn rewrite_shell_cmd(cmd: &str, cwd: &Path, eai_exe: &Path) -> Option<(String, &'static str)> {
    let trimmed = cmd.trim();
    if trimmed.is_empty()
        || trimmed.contains('|')
        || trimmed.contains("&&")
        || trimmed.contains(';')
        || trimmed.contains('\n')
        || trimmed.starts_with("squeeze ")
        || trimmed.starts_with("eai ")
    {
        return None;
    }

    let tokens = split_shell_tokens(trimmed);
    if tokens.is_empty() {
        return None;
    }

    let exe_str = eai_exe.display().to_string();
    // Don't double-wrap commands already invoking this eai binary
    if tokens[0] == exe_str || tokens[0].ends_with("/eai") || tokens[0].ends_with("\\eai.exe") {
        return None;
    }

    // 1. Redirect `cat <large_file>` (>300 lines) to `"<eai>" outline "<file>"`
    if tokens[0] == "cat" && tokens.len() == 2 && !tokens[1].starts_with('-') {
        let raw_p = PathBuf::from(&tokens[1]);
        let full_p = if raw_p.is_absolute() {
            raw_p
        } else {
            cwd.join(raw_p)
        };
        if full_p.is_file() && count_lines(&full_p) > 300 {
            let rewritten = format!(
                "{} outline {}",
                quote_for_shell(&exe_str),
                quote_for_shell(&tokens[1])
            );
            return Some((rewritten, "cat_to_outline"));
        }
    }

    // 2. Wrap noisy test/build suites with `"<eai>" squeeze -- <cmd>`
    if squeeze::rewriter::should_wrap_command(trimmed) {
        let rewritten = format!("{} squeeze -- {}", quote_for_shell(&exe_str), trimmed);
        return Some((rewritten, "wrap_squeeze"));
    }

    // 3. Apply compact H8 flags (git status -sb, git log --oneline -n 15, git --no-pager diff, etc.)
    let h8 = squeeze::rewriter::rewrite_command(trimmed);
    if h8 != trimmed {
        return Some((h8, "compact_flags"));
    }

    None
}

fn read_mode(cwd: &Path) -> String {
    let cfg = cwd.join(".eai").join("config.toml");
    if let Ok(txt) = fs::read_to_string(cfg) {
        for line in txt.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("read_mode") {
                if let Some((_, val)) = rest.split_once('=') {
                    return val.trim().trim_matches('"').trim_matches('\'').to_string();
                }
            }
        }
    }
    "gentle".to_string()
}

/// Handle Claude Code PreToolUse JSON payload.
pub fn handle_claude_payload(payload: &Value, eai_exe: &Path) -> Option<Value> {
    let tool_name = payload.get("tool_name").and_then(|v| v.as_str()).unwrap_or("");
    let tool_input = payload.get("tool_input")?.as_object()?;
    let cwd = payload
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    if matches!(tool_name, "Bash" | "PowerShell") {
        let orig_cmd = tool_input.get("command").and_then(|v| v.as_str()).unwrap_or("");
        if let Some((new_cmd, action)) = rewrite_shell_cmd(orig_cmd, &cwd, eai_exe) {
            record_hook_rewrite(&cwd, "claude", action);
            let mut updated = tool_input.clone();
            updated.insert("command".to_string(), Value::String(new_cmd));
            return Some(json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "allow",
                    "updatedInput": updated,
                }
            }));
        }
        return None;
    }

    if tool_name == "Read" {
        let fpath = tool_input.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
        if fpath.is_empty() {
            return None;
        }
        if is_eai_log_path(fpath) {
            record_recovery(&cwd, "claude");
            return None;
        }
        let has_range = tool_input.get("offset").is_some() || tool_input.get("limit").is_some();
        if !has_range {
            let raw_p = PathBuf::from(fpath);
            let full_p = if raw_p.is_absolute() { raw_p } else { cwd.join(&raw_p) };
            let lines = count_lines(&full_p);
            if lines > 350 {
                let mode = read_mode(&cwd);
                let fname = full_p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| fpath.to_string());
                if mode == "outline-first" {
                    // Opt-in outline-first mode: block FIRST unranged read with compact outline,
                    // allow immediately on second attempt (zero trap).
                    let seen_file = cwd.join(".eai").join("seen_reads.txt");
                    let key = full_p.display().to_string();
                    let already_seen = fs::read_to_string(&seen_file)
                        .map(|s| s.lines().any(|l| l == key))
                        .unwrap_or(false);
                    if !already_seen {
                        let _ = fs::create_dir_all(cwd.join(".eai"));
                        let mut existing = fs::read_to_string(&seen_file).unwrap_or_default();
                        existing.push_str(&key);
                        existing.push('\n');
                        let _ = fs::write(&seen_file, existing);
                        if let Ok((out, _)) = outline::format_file_outline(&full_p) {
                            record_hook_rewrite(&cwd, "claude", "outline_first_redirect");
                            return Some(json!({
                                "hookSpecificOutput": {
                                    "hookEventName": "PreToolUse",
                                    "permissionDecision": "deny",
                                    "permissionDecisionReason": format!(
                                        "[efficient-ai] {} has {} lines. Here is its symbol outline — use ranged Read (offset/limit) or retry Read to load the full file:\n{}",
                                        fname, lines, out
                                    )
                                }
                            }));
                        }
                    }
                } else {
                    // Default `gentle` mode: ultra-short 1-line nudge (~18 tok), NEVER dumps outline on top of full read
                    return Some(json!({
                        "systemMessage": format!(
                            "[eai] {} is {} lines; prefer `eai outline {}` or ranged Read (offset/limit) next time.",
                            fname, lines, fname
                        )
                    }));
                }
            }
        }
    }

    None
}

/// Handle Antigravity camelCase lifecycle hook payload (`PreToolUse`, `PostToolUse`, `Stop`).
pub fn handle_antigravity_payload(payload: &Value, eai_exe: &Path) -> Value {
    let event = payload
        .get("hookEventName")
        .and_then(|v| v.as_str())
        .unwrap_or("PreToolUse");
    let cwd = payload
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    match event {
        "PreToolUse" => {
            let Some(tc) = payload.get("toolCall").and_then(|v| v.as_object()) else {
                return json!({"decision": "allow"});
            };
            let tname = tc.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = tc.get("args").and_then(|v| v.as_object());

            if tname == "run_command" {
                let orig_cmd = args
                    .and_then(|a| a.get("CommandLine"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if let Some((new_cmd, action)) = rewrite_shell_cmd(orig_cmd, &cwd, eai_exe) {
                    record_hook_rewrite(&cwd, "antigravity", action);
                    return json!({
                        "decision": "allow",
                        "overwrite": {
                            "CommandLine": new_cmd
                        }
                    });
                }
            } else if tname == "read_file" {
                let fpath = args
                    .and_then(|a| a.get("Path"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if is_eai_log_path(fpath) {
                    record_recovery(&cwd, "antigravity");
                }
            }

            json!({"decision": "allow"})
        }
        "PostToolUse" => json!({}),
        "PreInvocation" | "PostInvocation" => json!({"injectSteps": []}),
        "Stop" => json!({"decision": "allow"}),
        _ => json!({}),
    }
}

/// Handle Gemini CLI `BeforeTool` and `AfterTool` hook payloads.
pub fn handle_gemini_payload(payload: &Value, eai_exe: &Path) -> Value {
    let event = payload
        .get("hook_event_name")
        .or_else(|| payload.get("hookEventName"))
        .and_then(|v| v.as_str())
        .unwrap_or("BeforeTool");
    let tool_name = payload.get("tool_name").and_then(|v| v.as_str()).unwrap_or("");
    let cwd = payload
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // AfterTool lossless compression for noisy shell outputs (Gemini CLI superpower!)
    if event == "AfterTool" || payload.get("tool_response").is_some() {
        if matches!(tool_name, "run_shell_command" | "shell" | "bash") {
            let raw_out = payload
                .get("tool_response")
                .and_then(|v| {
                    v.as_str()
                        .map(str::to_string)
                        .or_else(|| v.get("output").and_then(|o| o.as_str()).map(str::to_string))
                })
                .unwrap_or_default();

            if raw_out.lines().count() > 40 {
                let comp = squeeze::compressor::compress_stream(&raw_out, 150);
                if comp.elided_lines >= 10 {
                    let log_dir = cwd.join(".eai").join("logs");
                    let log_note = match squeeze::spill_to_log_in(&log_dir, &raw_out) {
                        Ok(p) => format!(
                            "\n[{} passing/duplicate lines elided ({} -> {} lines); full log: {}]",
                            comp.elided_lines,
                            comp.total_lines,
                            comp.kept_lines,
                            p.display()
                        ),
                        Err(_) => format!(
                            "\n[{} passing/duplicate lines elided ({} -> {} lines)]",
                            comp.elided_lines, comp.total_lines, comp.kept_lines
                        ),
                    };
                    let replaced = format!("{}{}", comp.output, log_note);
                    record_squeeze(
                        &cwd,
                        "gemini_after_tool",
                        raw_out.len(),
                        replaced.len(),
                        comp.elided_lines,
                        true,
                        false,
                    );
                    return json!({
                        "decision": "allow",
                        "hookSpecificOutput": {
                            "hookEventName": "AfterTool",
                            "additionalContext": "",
                        },
                        "systemMessage": "",
                        "tool_response": replaced
                    });
                }
            }
        }
        return json!({"decision": "allow"});
    }

    // BeforeTool command rewriting & recovery tracking
    if let Some(tool_input) = payload.get("tool_input").and_then(|v| v.as_object()) {
        if matches!(tool_name, "run_shell_command" | "shell" | "bash") {
            let orig_cmd = tool_input.get("command").and_then(|v| v.as_str()).unwrap_or("");
            if let Some((new_cmd, action)) = rewrite_shell_cmd(orig_cmd, &cwd, eai_exe) {
                record_hook_rewrite(&cwd, "gemini", action);
                let mut updated = tool_input.clone();
                updated.insert("command".to_string(), Value::String(new_cmd));
                return json!({
                    "decision": "allow",
                    "tool_input": updated
                });
            }
        } else if matches!(tool_name, "read_file" | "read_many_files") {
            let fpath = tool_input
                .get("path")
                .or_else(|| tool_input.get("file_path"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if is_eai_log_path(fpath) {
                record_recovery(&cwd, "gemini");
            }
        }
    }

    json!({"decision": "allow"})
}

pub fn cmd_hook(args: &[String]) -> i32 {
    let Some(agent) = args.first().map(|s| s.as_str()) else {
        eprintln!("Usage: eai hook <claude|antigravity|gemini>");
        return 2;
    };

    let mut raw = String::new();
    if io::stdin().read_to_string(&mut raw).is_err() || raw.trim().is_empty() {
        return 0;
    }
    let cleaned = raw.trim_start_matches('\u{feff}').trim();
    if cleaned.is_empty() {
        return 0;
    }
    let Ok(payload) = serde_json::from_str::<Value>(cleaned) else {
        return 0;
    };
    let eai_exe = env::current_exe().unwrap_or_else(|_| PathBuf::from("eai"));

    match agent {
        "claude" => {
            if let Some(out) = handle_claude_payload(&payload, &eai_exe) {
                println!("{}", out);
            }
            0
        }
        "antigravity" => {
            let out = handle_antigravity_payload(&payload, &eai_exe);
            println!("{}", out);
            0
        }
        "gemini" => {
            let out = handle_gemini_payload(&payload, &eai_exe);
            println!("{}", out);
            0
        }
        other => {
            eprintln!("Unknown hook agent '{}'. Expected: claude, antigravity, gemini", other);
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_hook_uses_exact_eai_binary_path() {
        let exe = Path::new("/opt/eai/bin/eai");
        let payload = json!({
            "tool_name": "Bash",
            "tool_input": {"command": "pytest tests/"},
            "cwd": "."
        });
        let out = handle_claude_payload(&payload, exe).expect("should rewrite");
        let cmd = out["hookSpecificOutput"]["updatedInput"]["command"]
            .as_str()
            .unwrap();
        assert_eq!(cmd, "/opt/eai/bin/eai squeeze -- pytest tests/");
    }

    #[test]
    fn test_antigravity_hook_overwrites_commandline() {
        let exe = Path::new("C:\\Program Files\\eai\\eai.exe");
        let payload = json!({
            "hookEventName": "PreToolUse",
            "cwd": ".",
            "toolCall": {
                "id": "t1",
                "name": "run_command",
                "args": {"CommandLine": "git status"}
            }
        });
        let out = handle_antigravity_payload(&payload, exe);
        assert_eq!(out["decision"], "allow");
        assert_eq!(out["overwrite"]["CommandLine"], "git status -sb");

        let payload2 = json!({
            "hookEventName": "PreToolUse",
            "cwd": ".",
            "toolCall": {
                "id": "t2",
                "name": "run_command",
                "args": {"CommandLine": "cargo test"}
            }
        });
        let out2 = handle_antigravity_payload(&payload2, exe);
        assert_eq!(
            out2["overwrite"]["CommandLine"],
            "\"C:\\Program Files\\eai\\eai.exe\" squeeze -- cargo test"
        );
    }

    #[test]
    fn test_gemini_hook_rewrites_and_allows() {
        let exe = Path::new("/usr/local/bin/eai");
        let payload = json!({
            "tool_name": "run_shell_command",
            "tool_input": {"command": "git log"}
        });
        let out = handle_gemini_payload(&payload, exe);
        assert_eq!(out["decision"], "allow");
        assert_eq!(out["tool_input"]["command"], "git log --oneline -n 15");
    }

    #[test]
    fn test_split_shell_tokens_with_spaces() {
        let t = split_shell_tokens("cat \"my folder/big file.py\"");
        assert_eq!(t, vec!["cat", "my folder/big file.py"]);
    }
}
