//! Zero-LLM-token live bottom status bar (`eai statusline`) for Antigravity CLI & Claude Code.
//!
//! Reads session JSON piped on `stdin` by either Antigravity CLI (`~/.gemini/antigravity-cli/settings.json`
//! with `stack_with_default: true`) or Claude Code (`~/.claude/settings.json`), merges local `.eai/metrics.jsonl`
//! savings/recovery/pause state + cached update status, and emits one width-responsive ANSI status line (<5ms).

use crate::pricing::resolve_base_usd_per_m;
use crate::setup::{
    detect_installed_agents, merge_antigravity_statusline, merge_claude_statusline,
    remove_eai_statusline,
};
use crate::telemetry::{read_events, summarize_events, user_home};
use crate::tokscope::detect_and_parse;
use crate::update::{cached_available_update, maybe_spawn_background_check, UpdateInfo};
use serde_json::Value;
use std::env;
use std::fs;
use std::io::{self, IsTerminal, Read};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct StatusSnapshot {
    pub is_antigravity: bool,
    pub model_label: Option<String>,
    pub paused: bool,
    pub saved_tokens: usize,
    pub saved_usd: f64,
    pub session_usd: Option<f64>,
    pub cache_hit_pct: Option<f64>,
    pub ctx_used_pct: Option<f64>,
    pub recovery_rate_pct: f64,
    pub recovery_events: usize,
    pub subagents: usize,
    pub update: Option<UpdateInfo>,
    pub width: usize,
    pub color: bool,
}

fn c(enabled: bool, code: &str, text: &str) -> String {
    if enabled {
        format!("\x1b[{}m{}\x1b[0m", code, text)
    } else {
        text.to_string()
    }
}

fn fmt_tok(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", (n as f64) / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", (n as f64) / 1_000.0)
    } else {
        format!("{}", n)
    }
}

fn fmt_bar(pct: f64, cells: usize) -> String {
    let clamped = pct.clamp(0.0, 100.0);
    let filled = ((clamped / 100.0) * (cells as f64)).round() as usize;
    let filled = filled.min(cells);
    let mut s = String::with_capacity(cells + 2);
    s.push('[');
    for _ in 0..filled {
        s.push('█');
    }
    for _ in filled..cells {
        s.push('░');
    }
    s.push(']');
    s
}

fn json_u64(v: Option<&Value>) -> u64 {
    v.and_then(|x| {
        x.as_u64()
            .or_else(|| x.as_f64().map(|f| f.max(0.0) as u64))
    })
    .unwrap_or(0)
}

fn json_f64(v: Option<&Value>) -> Option<f64> {
    v.and_then(|x| x.as_f64().or_else(|| x.as_u64().map(|u| u as f64)))
}

fn is_paused_at(cwd: &Path, home: Option<&Path>) -> bool {
    cwd.join(".eai").join("paused").is_file()
        || home
            .map(|h| h.join(".eai").join("paused").is_file())
            .unwrap_or(false)
}

fn short_model(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.len() <= 20 {
        return trimmed.to_string();
    }
    if let Some(pos) = trimmed.find('(') {
        return trimmed[..pos].trim().to_string();
    }
    trimmed.chars().take(18).collect()
}

/// Extract normalized status telemetry from either Antigravity CLI or Claude Code stdin JSON.
pub fn build_snapshot(
    payload: &Value,
    cwd_fallback: &Path,
    home: Option<&Path>,
    update: Option<UpdateInfo>,
    color: bool,
) -> StatusSnapshot {
    let is_antigravity = payload
        .get("product")
        .and_then(|v| v.as_str())
        .map(|s| s.eq_ignore_ascii_case("antigravity"))
        .unwrap_or(false)
        || payload.get("agent_state").is_some()
        || payload.get("conversation_id").is_some();

    let cwd = payload
        .get("cwd")
        .and_then(|v| v.as_str())
        .or_else(|| {
            payload
                .get("workspace")
                .and_then(|w| w.get("current_dir"))
                .and_then(|v| v.as_str())
        })
        .map(PathBuf::from)
        .unwrap_or_else(|| cwd_fallback.to_path_buf());

    let model_raw = payload
        .get("model")
        .and_then(|m| {
            m.get("display_name")
                .or_else(|| m.get("id"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("");
    let model_label = if model_raw.is_empty() {
        None
    } else {
        Some(short_model(model_raw))
    };

    let width = payload
        .get("terminal_width")
        .and_then(|v| v.as_u64())
        .map(|u| u as usize)
        .or_else(|| env::var("COLUMNS").ok().and_then(|s| s.parse().ok()))
        .unwrap_or(120);

    let cw = payload.get("context_window");
    let ctx_used_pct = json_f64(cw.and_then(|c| c.get("used_percentage"))).or_else(|| {
        let size = json_u64(cw.and_then(|c| c.get("context_window_size")));
        let cur_in = json_u64(
            cw.and_then(|c| c.get("current_usage"))
                .and_then(|u| u.get("input_tokens")),
        ) + json_u64(
            cw.and_then(|c| c.get("current_usage"))
                .and_then(|u| u.get("cache_read_input_tokens")),
        );
        if size > 0 && cur_in > 0 {
            Some(((cur_in as f64) / (size as f64) * 100.0).min(100.0))
        } else {
            None
        }
    });

    // Live cache-hit ratio from `context_window.current_usage` (supports both Claude & Gemini field names)
    let mut cache_hit_pct = cw.and_then(|c| c.get("current_usage")).and_then(|u| {
        let c_read = json_u64(
            u.get("cache_read_input_tokens")
                .or_else(|| u.get("cached_input_tokens"))
                .or_else(|| u.get("cached_content_token_count")),
        );
        let c_write = json_u64(u.get("cache_creation_input_tokens"));
        let uncached = json_u64(
            u.get("input_tokens")
                .or_else(|| u.get("prompt_token_count")),
        );
        let denom = c_read + c_write + uncached;
        if denom >= 256 {
            Some((c_read as f64) * 100.0 / (denom as f64))
        } else {
            None
        }
    });

    let mut session_usd = json_f64(
        payload
            .get("cost")
            .and_then(|c| c.get("total_cost_usd")),
    );

    // Optional fast transcript enrichment (only when <= 2 MB so statusline stays <8ms)
    if session_usd.is_none() || cache_hit_pct.is_none() {
        if let Some(tp) = payload.get("transcript_path").and_then(|v| v.as_str()) {
            let p = Path::new(tp);
            if let Ok(meta) = fs::metadata(p) {
                if meta.is_file() && meta.len() <= 2 * 1024 * 1024 {
                    let agent_hint = if is_antigravity { "antigravity" } else { "claude" };
                    if let Some(sess) = detect_and_parse(p, agent_hint) {
                        if session_usd.is_none() && sess.turn_count() > 0 {
                            session_usd = Some(sess.estimated_usd());
                        }
                        if cache_hit_pct.is_none() && sess.total_input() >= 256 {
                            cache_hit_pct = Some(sess.cache_hit_rate() * 100.0);
                        }
                    }
                }
            }
        }
    }

    // Fallback session USD from cumulative `context_window.total_input_tokens` / `total_output_tokens`
    let agent_name = if is_antigravity { "antigravity" } else { "claude" };
    let base_usd_per_m = resolve_base_usd_per_m(model_raw, agent_name);
    if session_usd.is_none() {
        let tot_in = json_u64(cw.and_then(|c| c.get("total_input_tokens")));
        let tot_out = json_u64(cw.and_then(|c| c.get("total_output_tokens")));
        if tot_in + tot_out > 0 {
            let est_in_w = if is_antigravity { 0.35 } else { 0.20 };
            let out_w = if is_antigravity { 4.0 } else { 5.0 };
            let cw_tok = (tot_in as f64) * est_in_w + (tot_out as f64) * out_w;
            session_usd = Some((cw_tok / 1_000_000.0) * base_usd_per_m);
        }
    }

    let subagents = payload
        .get("subagents")
        .map(|s| {
            s.as_array()
                .map(|a| a.len())
                .or_else(|| s.as_u64().map(|u| u as usize))
                .unwrap_or(0)
        })
        .unwrap_or(0);

    // Local `.eai/metrics.jsonl` (project first, falling back to global `~/.eai/metrics.jsonl`)
    let proj_metrics = cwd.join(".eai").join("metrics.jsonl");
    let mut events = read_events(&proj_metrics);
    if events.is_empty() {
        if let Some(h) = home {
            events = read_events(&h.join(".eai").join("metrics.jsonl"));
        }
    }
    let summary = summarize_events("project", &events);
    let saved_tokens = summary.total_tokens_saved;
    let saved_usd = ((saved_tokens as f64) / 1_000_000.0) * base_usd_per_m;

    StatusSnapshot {
        is_antigravity,
        model_label,
        paused: is_paused_at(&cwd, home),
        saved_tokens,
        saved_usd,
        session_usd,
        cache_hit_pct,
        ctx_used_pct,
        recovery_rate_pct: summary.recovery_rate_pct,
        recovery_events: summary.recoveries,
        subagents,
        update,
        width,
        color,
    }
}

/// Render a single terminal-width-responsive ANSI line.
pub fn format_statusline(s: &StatusSnapshot) -> String {
    let sep = c(s.color, "2", " · ");
    let mut parts: Vec<String> = Vec::new();
    let narrow = s.width < 84;
    let medium = s.width < 108;

    // On Claude Code, statusLine replaces the default footer so include model name first.
    // On Antigravity with stack_with_default=true, model/effort are already on the line above.
    if !s.is_antigravity {
        if let Some(ref m) = s.model_label {
            parts.push(c(s.color, "1;36", m));
        }
    }

    if s.paused {
        let lbl = if narrow {
            "eai PAUSED"
        } else {
            "eai PAUSED (eai resume)"
        };
        parts.push(c(s.color, "1;33", lbl));
    } else {
        parts.push(c(s.color, "1;32", "eai ON"));
    }

    if !s.paused {
        if s.saved_tokens > 0 {
            let txt = if narrow {
                format!("-{}", fmt_tok(s.saved_tokens))
            } else if s.saved_usd >= 0.01 {
                format!("saved {} (~${:.2})", fmt_tok(s.saved_tokens), s.saved_usd)
            } else {
                format!("saved {}", fmt_tok(s.saved_tokens))
            };
            parts.push(c(s.color, "32", &txt));
        } else if !narrow {
            parts.push(c(s.color, "2", "saved 0 tok"));
        }
    }

    if let Some(usd) = s.session_usd {
        let txt = if narrow {
            format!("${:.2}", usd)
        } else {
            format!("${:.2} sess", usd)
        };
        parts.push(c(s.color, "37", &txt));
    }

    if let Some(hit) = s.cache_hit_pct {
        if hit < 60.0 {
            let txt = if narrow {
                format!("BUST {:.0}%", hit)
            } else {
                format!("CACHE BUST {:.0}%", hit)
            };
            parts.push(c(s.color, "1;31", &txt));
        } else if hit < 80.0 {
            parts.push(c(s.color, "33", &format!("cache {:.0}%", hit)));
        } else {
            parts.push(c(s.color, "32", &format!("cache {:.0}%", hit)));
        }
    }

    if let Some(ctx) = s.ctx_used_pct {
        if ctx >= 85.0 {
            let txt = if narrow {
                format!("ctx {:.0}%!", ctx)
            } else {
                format!("ctx {:.0}% (/compact)", ctx)
            };
            parts.push(c(s.color, "1;31", &txt));
        } else if ctx >= 70.0 {
            let txt = if medium {
                format!("ctx {:.0}%", ctx)
            } else {
                format!("ctx {:.0}% {}", ctx, fmt_bar(ctx, 8))
            };
            parts.push(c(s.color, "33", &txt));
        } else {
            let txt = if medium {
                format!("ctx {:.0}%", ctx)
            } else {
                format!("ctx {:.0}% {}", ctx, fmt_bar(ctx, 8))
            };
            parts.push(c(s.color, "36", &txt));
        }
    }

    if !narrow && !s.paused {
        if s.recovery_events > 0 && s.recovery_rate_pct > 5.0 {
            parts.push(c(
                s.color,
                "33",
                &format!("rec {:.0}%", s.recovery_rate_pct),
            ));
        } else if !medium {
            parts.push(c(s.color, "2", &format!("rec {:.0}%", s.recovery_rate_pct)));
        }
    }

    if s.subagents > 0 {
        parts.push(c(s.color, "35", &format!("sub {}", s.subagents)));
    }

    if let Some(ref u) = s.update {
        let txt = if narrow {
            format!("up v{}", u.latest)
        } else {
            format!("update v{} (eai update)", u.latest)
        };
        parts.push(c(s.color, "1;33", &txt));
    }

    parts.join(&sep)
}

fn cmd_statusline_install(args: &[String], enable: bool) -> i32 {
    let mut agents_arg = "auto".to_string();
    let mut force = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--agents" if i + 1 < args.len() => {
                agents_arg = args[i + 1].clone();
                i += 1;
            }
            "--force" | "-f" => force = true,
            _ => {}
        }
        i += 1;
    }

    let Some(home) = user_home() else {
        eprintln!("Error: could not determine home directory.");
        return 1;
    };
    let eai_exe = env::current_exe().unwrap_or_else(|_| PathBuf::from("eai"));
    let detected = detect_installed_agents(Some(&home));

    let (want_claude, want_agy) = if agents_arg == "auto" {
        if detected.claude || detected.antigravity {
            (detected.claude, detected.antigravity)
        } else {
            (true, true)
        }
    } else {
        let low = agents_arg.to_ascii_lowercase();
        let all = low == "all";
        (
            all || low.contains("claude"),
            all || low.contains("antigravity") || low.contains("agy"),
        )
    };

    if enable {
        if want_agy {
            let p = home.join(".gemini").join("antigravity-cli").join("settings.json");
            match merge_antigravity_statusline(&p, &eai_exe, force) {
                Ok(true) => println!(
                    "- [OK] Antigravity CLI statusLine wired (stack_with_default=true): `{}`",
                    p.display()
                ),
                Ok(false) => println!(
                    "- [SKIP] Antigravity CLI already has a custom statusLine (pass `--force` to replace): `{}`",
                    p.display()
                ),
                Err(e) => eprintln!("- [WARN] Antigravity statusLine failed: {}", e),
            }
        }
        if want_claude {
            let p = home.join(".claude").join("settings.json");
            match merge_claude_statusline(&p, &eai_exe, force) {
                Ok(true) => println!("- [OK] Claude Code statusLine wired: `{}`", p.display()),
                Ok(false) => println!(
                    "- [SKIP] Claude Code already has a custom statusLine (pass `--force` to replace): `{}`",
                    p.display()
                ),
                Err(e) => eprintln!("- [WARN] Claude statusLine failed: {}", e),
            }
        }
    } else {
        if want_agy {
            let p = home.join(".gemini").join("antigravity-cli").join("settings.json");
            if let Ok(true) = remove_eai_statusline(&p) {
                println!("- [OK] Removed eai statusLine from `{}`", p.display());
            }
        }
        if want_claude {
            let p = home.join(".claude").join("settings.json");
            if let Ok(true) = remove_eai_statusline(&p) {
                println!("- [OK] Removed eai statusLine from `{}`", p.display());
            }
        }
    }
    0
}

fn cmd_statusline_preview(args: &[String]) -> i32 {
    let mut width: usize = 120;
    let mut color = env::var("NO_COLOR").is_err();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--width" if i + 1 < args.len() => {
                width = args[i + 1].parse().unwrap_or(120);
                i += 1;
            }
            "--no-color" => color = false,
            _ => {}
        }
        i += 1;
    }

    let sample_normal = StatusSnapshot {
        is_antigravity: true,
        model_label: Some("Gemini 4 Neon".into()),
        paused: false,
        saved_tokens: 48_200,
        saved_usd: 0.14,
        session_usd: Some(0.42),
        cache_hit_pct: Some(94.0),
        ctx_used_pct: Some(58.0),
        recovery_rate_pct: 0.0,
        recovery_events: 0,
        subagents: 0,
        update: None,
        width,
        color,
    };
    let sample_alert = StatusSnapshot {
        is_antigravity: true,
        model_label: Some("Gemini 4 Neon".into()),
        paused: false,
        saved_tokens: 19_400,
        saved_usd: 0.06,
        session_usd: Some(1.84),
        cache_hit_pct: Some(41.0),
        ctx_used_pct: Some(86.0),
        recovery_rate_pct: 12.0,
        recovery_events: 2,
        subagents: 2,
        update: Some(UpdateInfo {
            current: "1.1.0".into(),
            latest: "1.2.0".into(),
            release_url: String::new(),
        }),
        width,
        color,
    };
    let sample_claude = StatusSnapshot {
        is_antigravity: false,
        model_label: Some("Sonnet".into()),
        paused: false,
        saved_tokens: 61_500,
        saved_usd: 0.18,
        session_usd: Some(0.68),
        cache_hit_pct: Some(96.0),
        ctx_used_pct: Some(44.0),
        recovery_rate_pct: 0.0,
        recovery_events: 0,
        subagents: 0,
        update: None,
        width,
        color,
    };

    println!("Antigravity (stacked below default bar):");
    println!("  auto · Gemini 4 Neon · xhigh");
    println!("  {}", format_statusline(&sample_normal));
    println!("\nAntigravity (cache bust + high context + update alert):");
    println!("  {}", format_statusline(&sample_alert));
    println!("\nClaude Code:");
    println!("  {}", format_statusline(&sample_claude));
    0
}

pub fn cmd_statusline(args: &[String]) -> i32 {
    if let Some(first) = args.first().map(|s| s.as_str()) {
        match first {
            "install" | "on" | "enable" => return cmd_statusline_install(&args[1..], true),
            "off" | "disable" | "uninstall" => return cmd_statusline_install(&args[1..], false),
            "preview" => return cmd_statusline_preview(&args[1..]),
            "-h" | "--help" => {
                println!(
                    "Usage:\n\
                     \x20 eai statusline                       Render 1-line status bar from agent JSON on stdin\n\
                     \x20 eai statusline install [--force]     Wire statusLine in Antigravity CLI & Claude Code\n\
                     \x20 eai statusline off                   Remove eai statusLine from agent settings\n\
                     \x20 eai statusline preview [--width N]   Preview normal, alert & Claude Code views"
                );
                return 0;
            }
            _ => {}
        }
    }

    let no_color = args.iter().any(|a| a == "--no-color") || env::var("NO_COLOR").is_ok();
    maybe_spawn_background_check();
    let update = cached_available_update();

    let mut raw = String::new();
    if !io::stdin().is_terminal() {
        let _ = io::stdin().read_to_string(&mut raw);
    }

    let clean_raw = raw.trim_start_matches('\u{feff}').trim();
    let payload: Value = if clean_raw.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(clean_raw).unwrap_or_else(|_| serde_json::json!({}))
    };

    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let home = user_home();
    let snap = build_snapshot(&payload, &cwd, home.as_deref(), update, !no_color);
    println!("{}", format_statusline(&snap));
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_render_antigravity_normal_and_alerts() {
        let payload = json!({
            "product": "antigravity",
            "agent_state": "working",
            "cwd": ".",
            "terminal_width": 120,
            "model": { "display_name": "Gemini 4 Neon", "effort": "xhigh" },
            "context_window": {
                "used_percentage": 87,
                "current_usage": {
                    "input_tokens": 6000,
                    "cached_input_tokens": 4000
                }
            },
            "subagents": ["a", "b"]
        });
        let upd = Some(UpdateInfo {
            current: "1.1.0".into(),
            latest: "1.2.0".into(),
            release_url: String::new(),
        });
        let snap = build_snapshot(&payload, Path::new("."), None, upd, false);
        let line = format_statusline(&snap);
        assert!(line.contains("eai ON"));
        assert!(line.contains("CACHE BUST 40%"));
        assert!(line.contains("ctx 87% (/compact)"));
        assert!(line.contains("sub 2"));
        assert!(line.contains("update v1.2.0 (eai update)"));
        assert!(!line.contains("Gemini 4 Neon")); // stacked below default bar
    }

    #[test]
    fn test_render_claude_code_payload() {
        let payload = json!({
            "workspace": { "current_dir": "." },
            "model": { "id": "claude-sonnet-4-5", "display_name": "Sonnet" },
            "cost": { "total_cost_usd": 0.68 },
            "context_window": {
                "used_percentage": 44,
                "current_usage": {
                    "input_tokens": 200,
                    "cache_creation_input_tokens": 200,
                    "cache_read_input_tokens": 9600
                }
            }
        });
        let snap = build_snapshot(&payload, Path::new("."), None, None, false);
        let line = format_statusline(&snap);
        assert!(line.starts_with("Sonnet"));
        assert!(line.contains("$0.68 sess"));
        assert!(line.contains("cache 96%"));
        assert!(line.contains("ctx 44%"));
    }

    #[test]
    fn test_render_narrow_width() {
        let snap = StatusSnapshot {
            is_antigravity: true,
            paused: false,
            saved_tokens: 48_200,
            saved_usd: 0.14,
            session_usd: Some(0.42),
            cache_hit_pct: Some(94.0),
            ctx_used_pct: Some(58.0),
            update: Some(UpdateInfo {
                current: "1.1.0".into(),
                latest: "1.2.0".into(),
                release_url: String::new(),
            }),
            width: 72,
            color: false,
            ..Default::default()
        };
        let line = format_statusline(&snap);
        assert!(line.contains("-48.2k"));
        assert!(line.contains("up v1.2.0"));
        assert!(!line.contains('['));
    }
}
