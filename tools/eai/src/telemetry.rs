//! Lightweight local telemetry ledger (`.eai/metrics.jsonl`), Pre-EAI baseline snapshots,
//! and `eai stats` reporting. 100% local — never sends data over the network.

use serde::{Deserialize, Serialize};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricEvent {
    pub ts: u64,
    pub tool: String, // "outline", "squeeze", "hook_rewrite", "recovery"
    pub action: String,
    pub raw_tokens_est: usize,
    pub kept_tokens_est: usize,
    pub saved_tokens_est: usize,
    #[serde(default)]
    pub elided_lines: usize,
    #[serde(default)]
    pub loop_detected: bool,
    #[serde(default)]
    pub spilled_log: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BaselineSnapshot {
    pub captured_at: u64,
    pub sessions_sampled: usize,
    pub total_turns: usize,
    pub avg_cost_weighted_per_session: f64,
    pub avg_usd_per_session: f64,
    pub cache_hit_rate: f64,
    pub whole_file_read_ratio: f64,
    pub static_prefix_tokens: usize,
}

pub fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn est_tokens_from_chars(chars: usize) -> usize {
    if chars == 0 {
        0
    } else {
        ((chars as f64) / 3.8).round().max(1.0) as usize
    }
}

pub fn user_home() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Find the `.eai` directory for telemetry without cluttering uninitialized directories:
/// writes to `<base>/.eai/metrics.jsonl` if `<base>/.eai` exists, and always updates `~/.eai/metrics.jsonl`
/// if `~/.eai` exists.
fn telemetry_targets(base_dir: &Path) -> Vec<PathBuf> {
    if env::var("EAI_TELEMETRY").ok().as_deref() == Some("0") {
        return Vec::new();
    }
    let mut targets = Vec::new();
    let local_eai = base_dir.join(".eai");
    if local_eai.is_dir() {
        targets.push(local_eai.join("metrics.jsonl"));
    }
    if let Some(home) = user_home() {
        let global_eai = home.join(".eai");
        if global_eai.is_dir() {
            let g_path = global_eai.join("metrics.jsonl");
            if !targets.contains(&g_path) {
                targets.push(g_path);
            }
        }
    }
    targets
}

pub fn append_event(base_dir: &Path, event: &MetricEvent) {
    let Ok(line) = serde_json::to_string(event) else {
        return;
    };
    for path in telemetry_targets(base_dir) {
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(f, "{}", line);
        }
    }
}

pub fn record_outline(base_dir: &Path, action: &str, raw_chars: usize, out_chars: usize) {
    let raw_tok = est_tokens_from_chars(raw_chars);
    let kept_tok = est_tokens_from_chars(out_chars);
    let saved = raw_tok.saturating_sub(kept_tok);
    append_event(
        base_dir,
        &MetricEvent {
            ts: now_unix_secs(),
            tool: "outline".to_string(),
            action: action.to_string(),
            raw_tokens_est: raw_tok,
            kept_tokens_est: kept_tok,
            saved_tokens_est: saved,
            elided_lines: 0,
            loop_detected: false,
            spilled_log: false,
        },
    );
}

pub fn record_squeeze(
    base_dir: &Path,
    cmd: &str,
    raw_chars: usize,
    kept_chars: usize,
    elided_lines: usize,
    spilled_log: bool,
    loop_detected: bool,
) {
    let raw_tok = est_tokens_from_chars(raw_chars);
    let kept_tok = est_tokens_from_chars(kept_chars);
    let saved = raw_tok.saturating_sub(kept_tok);
    let prog = cmd.split_whitespace().next().unwrap_or("cmd").to_string();
    append_event(
        base_dir,
        &MetricEvent {
            ts: now_unix_secs(),
            tool: "squeeze".to_string(),
            action: prog,
            raw_tokens_est: raw_tok,
            kept_tokens_est: kept_tok,
            saved_tokens_est: saved,
            elided_lines,
            loop_detected,
            spilled_log,
        },
    );
}

pub fn record_hook_rewrite(base_dir: &Path, agent: &str, action: &str) {
    append_event(
        base_dir,
        &MetricEvent {
            ts: now_unix_secs(),
            tool: "hook_rewrite".to_string(),
            action: format!("{}:{}", agent, action),
            raw_tokens_est: 0,
            kept_tokens_est: 0,
            saved_tokens_est: 0,
            elided_lines: 0,
            loop_detected: false,
            spilled_log: false,
        },
    );
}

pub fn record_recovery(base_dir: &Path, agent: &str) {
    append_event(
        base_dir,
        &MetricEvent {
            ts: now_unix_secs(),
            tool: "recovery".to_string(),
            action: agent.to_string(),
            raw_tokens_est: 0,
            kept_tokens_est: 0,
            saved_tokens_est: 0,
            elided_lines: 0,
            loop_detected: false,
            spilled_log: false,
        },
    );
}

pub fn read_events(metrics_file: &Path) -> Vec<MetricEvent> {
    let Ok(txt) = fs::read_to_string(metrics_file) else {
        return Vec::new();
    };
    txt.lines()
        .filter_map(|l| serde_json::from_str::<MetricEvent>(l).ok())
        .collect()
}

pub fn write_baseline(project_dir: &Path, snap: &BaselineSnapshot) -> std::io::Result<PathBuf> {
    let dir = project_dir.join(".eai");
    fs::create_dir_all(&dir)?;
    let path = dir.join("baseline.json");
    let body = serde_json::to_string_pretty(snap).unwrap_or_default();
    fs::write(&path, format!("{}\n", body))?;
    Ok(path)
}

pub fn read_baseline(project_dir: &Path) -> Option<BaselineSnapshot> {
    let path = project_dir.join(".eai").join("baseline.json");
    let txt = fs::read_to_string(path).ok()?;
    serde_json::from_str(&txt).ok()
}

#[derive(Debug, Default, Serialize)]
pub struct StatsSummary {
    pub scope: String,
    pub total_events: usize,
    pub outline_calls: usize,
    pub outline_tokens_saved: usize,
    pub squeeze_calls: usize,
    pub squeeze_tokens_saved: usize,
    pub squeeze_lines_elided: usize,
    pub squeeze_spilled_logs: usize,
    pub hook_rewrites: usize,
    pub loops_intercepted: usize,
    pub recoveries: usize,
    pub recovery_rate_pct: f64,
    pub total_tokens_saved: usize,
    pub estimated_resent_tokens_avoided: usize,
}

pub fn summarize_events(scope: &str, events: &[MetricEvent]) -> StatsSummary {
    let mut s = StatsSummary {
        scope: scope.to_string(),
        total_events: events.len(),
        ..Default::default()
    };
    for ev in events {
        match ev.tool.as_str() {
            "outline" => {
                s.outline_calls += 1;
                s.outline_tokens_saved += ev.saved_tokens_est;
            }
            "squeeze" => {
                s.squeeze_calls += 1;
                s.squeeze_tokens_saved += ev.saved_tokens_est;
                s.squeeze_lines_elided += ev.elided_lines;
                if ev.spilled_log {
                    s.squeeze_spilled_logs += 1;
                }
                if ev.loop_detected {
                    s.loops_intercepted += 1;
                }
            }
            "hook_rewrite" => {
                s.hook_rewrites += 1;
            }
            "recovery" => {
                s.recoveries += 1;
            }
            _ => {}
        }
    }
    s.total_tokens_saved = s.outline_tokens_saved + s.squeeze_tokens_saved;
    // Using Exp 003 median history amplification (~54x average across multi-turn sessions; conservative 25x estimate)
    s.estimated_resent_tokens_avoided = s.total_tokens_saved * 25;
    s.recovery_rate_pct = if s.squeeze_spilled_logs > 0 {
        100.0 * (s.recoveries as f64) / (s.squeeze_spilled_logs as f64)
    } else {
        0.0
    };
    s
}

pub fn cmd_stats(args: &[String]) -> i32 {
    let mut global = false;
    let mut json_out = false;
    let mut project = PathBuf::from(".");

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--global" | "-g" => global = true,
            "--json" => json_out = true,
            "--project" | "-p" if i + 1 < args.len() => {
                project = PathBuf::from(&args[i + 1]);
                i += 1;
            }
            "-h" | "--help" => {
                println!(
                    "Usage: eai stats [--project <dir>] [--global] [--json]\n\
                     Show local efficient-ai savings, recovery rate, stuck loops prevented, and baseline snapshot."
                );
                return 0;
            }
            _ => {}
        }
        i += 1;
    }

    let (scope, metrics_path) = if global {
        let Some(home) = user_home() else {
            eprintln!("Could not determine home directory.");
            return 1;
        };
        ("global (~/.eai)".to_string(), home.join(".eai").join("metrics.jsonl"))
    } else {
        let resolved = fs::canonicalize(&project).unwrap_or(project.clone());
        let label = resolved
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".".to_string());
        (label, project.join(".eai").join("metrics.jsonl"))
    };

    let events = read_events(&metrics_path);
    let summary = summarize_events(&scope, &events);
    let baseline = if global { None } else { read_baseline(&project) };

    if json_out {
        let payload = serde_json::json!({
            "stats": summary,
            "baseline": baseline,
        });
        println!("{}", serde_json::to_string_pretty(&payload).unwrap_or_default());
        return 0;
    }

    println!("# efficient-ai Local Telemetry — {}\n", summary.scope);
    if summary.total_events == 0 {
        println!("No local events recorded yet at `{}`.", metrics_path.display());
        println!("Run `eai init` in this project, then use your agent normally — savings & recovery rate will track automatically.");
    } else {
        println!("| Metric | Value |");
        println!("|---|---:|");
        println!(
            "| `outline` calls / direct tokens saved | {} calls / **{}** tok |",
            summary.outline_calls, summary.outline_tokens_saved
        );
        println!(
            "| `squeeze` calls / direct tokens saved | {} calls / **{}** tok ({} lines elided) |",
            summary.squeeze_calls, summary.squeeze_tokens_saved, summary.squeeze_lines_elided
        );
        println!("| Hook compact-flag rewrites | {} |", summary.hook_rewrites);
        println!(
            "| **Total direct tokens saved** | **{}** tok (~{} re-sent tok avoided @ 25x) |",
            summary.total_tokens_saved, summary.estimated_resent_tokens_avoided
        );
        println!(
            "| Quality guardrail — `squeeze` recovery rate | **{:.1}%** ({}/{} spilled logs opened) |",
            summary.recovery_rate_pct, summary.recoveries, summary.squeeze_spilled_logs
        );
        println!("| Stuck loops intercepted (H7) | {} |", summary.loops_intercepted);
    }

    if let Some(b) = baseline {
        println!("\n## Pre-EAI Baseline Snapshot (`.eai/baseline.json`)\n");
        println!(
            "- **Historical sessions sampled:** {} ({} turns)",
            b.sessions_sampled, b.total_turns
        );
        println!(
            "- **Baseline avg cost / session:** ${:.3} ({:.0} cost-weighted tok)",
            b.avg_usd_per_session, b.avg_cost_weighted_per_session
        );
        println!(
            "- **Baseline cache hit rate:** {:.1}% | **Whole-file read ratio:** {:.1}%",
            b.cache_hit_rate * 100.0,
            b.whole_file_read_ratio * 100.0
        );
        println!(
            "- **Baseline idle static prefix:** {} tok/turn",
            b.static_prefix_tokens
        );
    }

    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summarize_events_and_recovery_rate() {
        let events = vec![
            MetricEvent {
                ts: 1,
                tool: "outline".into(),
                action: "outline".into(),
                raw_tokens_est: 2000,
                kept_tokens_est: 200,
                saved_tokens_est: 1800,
                elided_lines: 0,
                loop_detected: false,
                spilled_log: false,
            },
            MetricEvent {
                ts: 2,
                tool: "squeeze".into(),
                action: "pytest".into(),
                raw_tokens_est: 1200,
                kept_tokens_est: 100,
                saved_tokens_est: 1100,
                elided_lines: 40,
                loop_detected: true,
                spilled_log: true,
            },
            MetricEvent {
                ts: 3,
                tool: "squeeze".into(),
                action: "cargo".into(),
                raw_tokens_est: 900,
                kept_tokens_est: 100,
                saved_tokens_est: 800,
                elided_lines: 30,
                loop_detected: false,
                spilled_log: true,
            },
            MetricEvent {
                ts: 4,
                tool: "recovery".into(),
                action: "claude".into(),
                raw_tokens_est: 0,
                kept_tokens_est: 0,
                saved_tokens_est: 0,
                elided_lines: 0,
                loop_detected: false,
                spilled_log: false,
            },
        ];
        let s = summarize_events("demo", &events);
        assert_eq!(s.total_tokens_saved, 3700);
        assert_eq!(s.loops_intercepted, 1);
        assert_eq!(s.squeeze_spilled_logs, 2);
        assert_eq!(s.recoveries, 1);
        assert!((s.recovery_rate_pct - 50.0).abs() < 0.01);
    }
}
