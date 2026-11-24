//! Before/After Project Impact Proof (`eai impact`), shareable privacy-safe SVG proof cards
//! (`eai share`), and live session watcher (`eai watch`).

use crate::telemetry::{
    read_baseline, read_events, summarize_events, BaselineSnapshot, StatsSummary,
};
use crate::tokscope::{
    always_prefix_tokens, audit_directory, compute_advisor_tips, detect_and_parse,
    load_sessions_window, newest_transcript_file, parse_since_days, summarize_corpus, AdvisorTip,
    CorpusSummary,
};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

#[derive(Debug, Serialize)]
pub struct BeforeAfterDelta {
    pub metric: String,
    pub before: String,
    pub after: String,
    pub delta: String,
    pub improved: bool,
}

#[derive(Debug, Serialize)]
pub struct ImpactReport {
    pub project_name: String,
    pub window_days: u64,
    pub has_baseline: bool,
    pub baseline: Option<BaselineSnapshot>,
    pub current_sessions: usize,
    pub current_turns: usize,
    pub current_avg_usd_per_session: f64,
    pub current_avg_cw_per_session: f64,
    pub current_cache_hit_pct: f64,
    pub current_whole_read_pct: f64,
    pub current_static_prefix_tokens: usize,
    pub total_spend_usd: f64,
    pub est_direct_usd_saved: f64,
    pub local_stats: StatsSummary,
    pub comparisons: Vec<BeforeAfterDelta>,
    pub advisor_tips: Vec<AdvisorTip>,
    pub agents_active: Vec<String>,
}

pub fn build_impact_report(
    project_dir: &Path,
    window_days: u64,
    agent_filter: &str,
    max_sessions: Option<usize>,
) -> (ImpactReport, CorpusSummary) {
    let resolved = fs::canonicalize(project_dir).unwrap_or_else(|_| project_dir.to_path_buf());
    let project_name = resolved
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "workspace".to_string());

    let baseline = read_baseline(&resolved);
    let events = read_events(&resolved.join(".eai").join("metrics.jsonl"));
    let local_stats = summarize_events(&project_name, &events);
    let audit_items = audit_directory(&resolved, false);
    let current_prefix = always_prefix_tokens(&audit_items);

    let sessions = load_sessions_window(None, agent_filter, Some(window_days), max_sessions);

    // If a baseline exists, split recent sessions after `baseline.captured_at` when available;
    // otherwise use the full window for current performance telemetry.
    let post_sessions: Vec<_> = if let Some(ref b) = baseline {
        let filtered: Vec<_> = sessions
            .iter()
            .filter(|s| s.mtime_secs >= b.captured_at)
            .cloned()
            .collect();
        if filtered.is_empty() {
            sessions.clone()
        } else {
            filtered
        }
    } else {
        sessions.clone()
    };

    let summary = summarize_corpus(&post_sessions);
    let denom = summary.total_files.max(1) as f64;
    let avg_usd = summary.total_estimated_usd / denom;
    let avg_cw = summary.total_cost_weighted / denom;
    let cache_pct = summary.cache_hit_rate() * 100.0;
    let tot_reads = summary.read_full_count + summary.read_ranged_count;
    let whole_read_pct = if tot_reads > 0 {
        100.0 * (summary.read_full_count as f64) / (tot_reads as f64)
    } else {
        0.0
    };

    // Conservative USD estimate for direct + cached-resent tokens avoided via local outline/squeeze
    // Each saved token avoids 1x write (~1.25x) + ~24x cached reads (~2.4x) = ~3.65x input-eq @ ~$2.25/M avg
    let avoided_input_eq = (local_stats.total_tokens_saved as f64) * 3.65;
    let est_direct_usd_saved = (avoided_input_eq / 1_000_000.0) * 2.25;

    let mut comparisons = Vec::new();
    if let Some(ref b) = baseline {
        if b.avg_usd_per_session > 0.001 {
            let pct = 100.0 * (avg_usd - b.avg_usd_per_session) / b.avg_usd_per_session;
            comparisons.push(BeforeAfterDelta {
                metric: "Avg cost / session (USD)".into(),
                before: format!("${:.3}", b.avg_usd_per_session),
                after: format!("${:.3}", avg_usd),
                delta: format!("{:+.1}%", pct),
                improved: pct <= 0.0,
            });
        }
        if b.avg_cost_weighted_per_session > 1.0 {
            let pct = 100.0 * (avg_cw - b.avg_cost_weighted_per_session)
                / b.avg_cost_weighted_per_session;
            comparisons.push(BeforeAfterDelta {
                metric: "Cost-weighted tok / session".into(),
                before: fmt_compact(b.avg_cost_weighted_per_session as u64),
                after: fmt_compact(avg_cw as u64),
                delta: format!("{:+.1}%", pct),
                improved: pct <= 0.0,
            });
        }
        let b_cache = b.cache_hit_rate * 100.0;
        let d_cache = cache_pct - b_cache;
        comparisons.push(BeforeAfterDelta {
            metric: "Prompt-cache hit rate".into(),
            before: format!("{:.1}%", b_cache),
            after: format!("{:.1}%", cache_pct),
            delta: format!("{:+.1} pp", d_cache),
            improved: d_cache >= -0.5,
        });
        let b_read = b.whole_file_read_ratio * 100.0;
        let d_read = whole_read_pct - b_read;
        comparisons.push(BeforeAfterDelta {
            metric: "Whole-file Read ratio".into(),
            before: format!("{:.1}%", b_read),
            after: format!("{:.1}%", whole_read_pct),
            delta: format!("{:+.1} pp", d_read),
            improved: d_read <= 0.0,
        });
        if b.static_prefix_tokens > 0 {
            let pct = 100.0 * ((current_prefix as f64) - (b.static_prefix_tokens as f64))
                / (b.static_prefix_tokens as f64);
            comparisons.push(BeforeAfterDelta {
                metric: "Idle static prefix / turn".into(),
                before: format!("{} tok", b.static_prefix_tokens),
                after: format!("{} tok", current_prefix),
                delta: format!("{:+.1}%", pct),
                improved: pct <= 0.0,
            });
        }
    }

    let advisor_tips =
        compute_advisor_tips(&summary, current_prefix, local_stats.recovery_rate_pct);
    let agents_active: Vec<String> = summary.by_agent.keys().cloned().collect();

    let report = ImpactReport {
        project_name,
        window_days,
        has_baseline: baseline.is_some(),
        baseline,
        current_sessions: summary.total_files,
        current_turns: summary.total_turns(),
        current_avg_usd_per_session: avg_usd,
        current_avg_cw_per_session: avg_cw,
        current_cache_hit_pct: cache_pct,
        current_whole_read_pct: whole_read_pct,
        current_static_prefix_tokens: current_prefix,
        total_spend_usd: summary.total_estimated_usd,
        est_direct_usd_saved,
        local_stats,
        comparisons,
        advisor_tips,
        agents_active,
    };

    (report, summary)
}

pub fn fmt_compact(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.2}B", (n as f64) / 1_000_000_000.0)
    } else if n >= 1_000_000 {
        format!("{:.1}M", (n as f64) / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", (n as f64) / 1_000.0)
    } else {
        n.to_string()
    }
}

pub fn cmd_impact(args: &[String]) -> i32 {
    let mut project = PathBuf::from(".");
    let mut since_days: u64 = 30;
    let mut agent = "auto".to_string();
    let mut json_out = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--project" | "-p" if i + 1 < args.len() => {
                project = PathBuf::from(&args[i + 1]);
                i += 1;
            }
            "--since" if i + 1 < args.len() => {
                since_days = parse_since_days(&args[i + 1]).unwrap_or(30);
                i += 1;
            }
            "--agent" if i + 1 < args.len() => {
                agent = args[i + 1].clone();
                i += 1;
            }
            "--json" => json_out = true,
            "-h" | "--help" => {
                println!(
                    "Usage: eai impact [--project <dir>] [--since 30d] [--agent auto] [--json]\n\
                     Show Before vs. After efficient-ai performance proof, savings, quality guardrails, and optimization tips."
                );
                return 0;
            }
            _ => {}
        }
        i += 1;
    }

    let (report, _summary) = build_impact_report(&project, since_days, &agent, Some(250));
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());
        return 0;
    }

    println!(
        "# efficient-ai Impact Report — {} (last {} days)\n",
        report.project_name, report.window_days
    );

    if !report.comparisons.is_empty() {
        println!("## 1. Before vs. After `efficient-ai`\n");
        println!("| Metric | Before EAI | With EAI | Delta |");
        println!("|---|---:|---:|---:|");
        for row in &report.comparisons {
            println!(
                "| {} | {} | {} | **{}** |",
                row.metric, row.before, row.after, row.delta
            );
        }
        println!();
    } else {
        println!(
            "*No Pre-EAI baseline found yet. Run `eai init` in `{}` to capture a baseline snapshot for Before vs. After comparisons.*\n",
            project.display()
        );
    }

    println!("## 2. Live Performance & Quality Guardrails\n");
    println!(
        "- **Sessions analyzed ({}d):** {} ({} API turns, active agents: {})",
        report.window_days,
        report.current_sessions,
        report.current_turns,
        if report.agents_active.is_empty() {
            "none".to_string()
        } else {
            report.agents_active.join(", ")
        }
    );
    println!(
        "- **Estimated spend ({}d):** **${:.2} USD** (avg ${:.3}/session)",
        report.window_days, report.total_spend_usd, report.current_avg_usd_per_session
    );
    println!(
        "- **Prompt-cache hit rate:** **{:.1}%** | **Whole-file Read ratio:** **{:.1}%** | **Idle prefix:** **{} tok/turn**",
        report.current_cache_hit_pct,
        report.current_whole_read_pct,
        report.current_static_prefix_tokens
    );
    println!(
        "- **Direct local savings (`outline` + `squeeze`):** **{} tok** (~{} re-sent tok avoided, ~**${:.2} USD**)",
        fmt_compact(report.local_stats.total_tokens_saved as u64),
        fmt_compact(report.local_stats.estimated_resent_tokens_avoided as u64),
        report.est_direct_usd_saved
    );
    println!(
        "- **Squeeze recovery rate (quality guardrail):** **{:.1}%** ({}/{} spilled logs opened) | **Stuck loops blocked:** {}",
        report.local_stats.recovery_rate_pct,
        report.local_stats.recoveries,
        report.local_stats.squeeze_spilled_logs,
        report.local_stats.loops_intercepted
    );

    if !report.advisor_tips.is_empty() {
        println!("\n## 3. Optimization Advisor\n");
        for tip in &report.advisor_tips {
            let badge = match tip.severity.as_str() {
                "high" => "[ACTION]",
                "medium" => "[TUNE]",
                _ => "[OK]",
            };
            println!("- **{} {}** — {}", badge, tip.title, tip.detail);
        }
    }

    0
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Render a privacy-safe, June-aesthetic 880x440 SVG Efficiency Proof Card (zero paths/code/prompts).
pub fn render_proof_card_svg(report: &ImpactReport) -> String {
    let agents_label = if report.agents_active.is_empty() {
        "Claude · Antigravity · Gemini · Codex".to_string()
    } else {
        report
            .agents_active
            .iter()
            .map(|a| match a.as_str() {
                "claude" => "Claude Code",
                "antigravity" => "Antigravity",
                "gemini" => "Gemini CLI",
                "codex" => "Codex CLI",
                other => other,
            })
            .collect::<Vec<_>>()
            .join(" · ")
    };

    let saved_tok = fmt_compact(report.local_stats.estimated_resent_tokens_avoided as u64);
    let direct_tok = fmt_compact(report.local_stats.total_tokens_saved as u64);
    let cache_str = format!("{:.1}%", report.current_cache_hit_pct);
    let recovery_str = format!("{:.1}%", report.local_stats.recovery_rate_pct);
    let ranged_pct = (100.0 - report.current_whole_read_pct).clamp(0.0, 100.0);
    let ranged_str = format!("{:.0}%", ranged_pct);

    let out_tok = report.local_stats.outline_tokens_saved as f64;
    let sqz_tok = report.local_stats.squeeze_tokens_saved as f64;
    let max_bar = out_tok.max(sqz_tok).max(1.0);
    let out_w = ((out_tok / max_bar) * 320.0).max(12.0) as usize;
    let sqz_w = ((sqz_tok / max_bar) * 320.0).max(12.0) as usize;
    let cache_w = ((report.current_cache_hit_pct / 100.0) * 320.0).clamp(12.0, 320.0) as usize;

    let subtitle = format!(
        "{}-Day Verified AI Efficiency Proof · {} Sessions · {} Turns",
        report.window_days,
        report.current_sessions,
        fmt_compact(report.current_turns as u64)
    );

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 880 440" width="880" height="440" font-family="'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif">
  <defs>
    <radialGradient id="glow" cx="50%" cy="0%" r="70%">
      <stop offset="0%" stop-color="#625DF5" stop-opacity="0.16"/>
      <stop offset="100%" stop-color="#F6F7FB" stop-opacity="0"/>
    </radialGradient>
    <linearGradient id="accent" x1="0%" y1="0%" x2="100%" y2="0%">
      <stop offset="0%" stop-color="#5B54FA"/>
      <stop offset="100%" stop-color="#8B5CF6"/>
    </linearGradient>
    <filter id="shadow" x="-4%" y="-6%" width="108%" height="116%">
      <feDropShadow dx="0" dy="6" stdDeviation="10" flood-color="#151531" flood-opacity="0.06"/>
    </filter>
  </defs>

  <rect width="880" height="440" rx="22" fill="#F6F7FB"/>
  <rect width="880" height="440" rx="22" fill="url(#glow)"/>
  <rect x="1" y="1" width="878" height="438" rx="21" fill="none" stroke="#E2E5F1" stroke-width="1.5"/>

  <!-- Header -->
  <rect x="34" y="28" width="10" height="36" rx="5" fill="url(#accent)"/>
  <text x="56" y="47" font-size="22" font-weight="800" fill="#151531">efficient-ai</text>
  <text x="188" y="47" font-size="14" font-weight="600" fill="#5B54FA">Same quality, fewer tokens</text>
  <text x="56" y="68" font-size="12.5" fill="#5E6478">{subtitle}</text>

  <rect x="556" y="30" width="290" height="32" rx="16" fill="#FFFFFF" stroke="#E2E5F1"/>
  <text x="701" y="51" font-size="11.5" font-weight="600" fill="#3B4058" text-anchor="middle">{agents}</text>

  <!-- 4 KPI Tiles -->
  <g filter="url(#shadow)">
    <rect x="34" y="92" width="192" height="108" rx="14" fill="#FFFFFF"/>
    <text x="52" y="118" font-size="11" font-weight="700" fill="#6B7280" letter-spacing="0.6">RE-SENT TOKENS AVOIDED</text>
    <text x="52" y="156" font-size="30" font-weight="800" fill="#5B54FA">{saved_tok}</text>
    <text x="52" y="180" font-size="11.5" fill="#10B981" font-weight="600">{direct_tok} direct saved (~${est_saved:.2})</text>
  </g>

  <g filter="url(#shadow)">
    <rect x="241" y="92" width="192" height="108" rx="14" fill="#FFFFFF"/>
    <text x="259" y="118" font-size="11" font-weight="700" fill="#6B7280" letter-spacing="0.6">PROMPT-CACHE HIT RATE</text>
    <text x="259" y="156" font-size="30" font-weight="800" fill="#151531">{cache_str}</text>
    <text x="259" y="180" font-size="11.5" fill="#10B981" font-weight="600">Prefix stability protected</text>
  </g>

  <g filter="url(#shadow)">
    <rect x="448" y="92" width="192" height="108" rx="14" fill="#FFFFFF"/>
    <text x="466" y="118" font-size="11" font-weight="700" fill="#6B7280" letter-spacing="0.6">TARGETED READ RATIO</text>
    <text x="466" y="156" font-size="30" font-weight="800" fill="#151531">{ranged_str}</text>
    <text x="466" y="180" font-size="11.5" fill="#5E6478">Idle prefix: {prefix_tok} tok/turn</text>
  </g>

  <g filter="url(#shadow)">
    <rect x="655" y="92" width="191" height="108" rx="14" fill="#FFFFFF"/>
    <text x="673" y="118" font-size="11" font-weight="700" fill="#6B7280" letter-spacing="0.6">RECOVERY RATE</text>
    <text x="673" y="156" font-size="30" font-weight="800" fill="#10B981">{recovery_str}</text>
    <text x="673" y="180" font-size="11.5" fill="#5E6478">{loops} stuck loops intercepted</text>
  </g>

  <!-- Efficiency Breakdown Panel -->
  <g filter="url(#shadow)">
    <rect x="34" y="220" width="812" height="162" rx="16" fill="#FFFFFF"/>
    <text x="56" y="250" font-size="13.5" font-weight="700" fill="#151531">Layer Impact Breakdown (Privacy-Sanitized Telemetry)</text>

    <text x="56" y="284" font-size="12" font-weight="600" fill="#3B4058">outline (Reads)</text>
    <rect x="210" y="272" width="320" height="14" rx="7" fill="#EEF0FA"/>
    <rect x="210" y="272" width="{out_w}" height="14" rx="7" fill="#5B54FA"/>
    <text x="548" y="284" font-size="12" font-weight="700" fill="#151531">{out_calls} calls · {out_saved} tok saved</text>

    <text x="56" y="318" font-size="12" font-weight="600" fill="#3B4058">squeeze (Streams)</text>
    <rect x="210" y="306" width="320" height="14" rx="7" fill="#EEF0FA"/>
    <rect x="210" y="306" width="{sqz_w}" height="14" rx="7" fill="#10B981"/>
    <text x="548" y="318" font-size="12" font-weight="700" fill="#151531">{sqz_calls} runs · {sqz_saved} tok ({sqz_lines} lines elided)</text>

    <text x="56" y="352" font-size="12" font-weight="600" fill="#3B4058">Cache Efficiency</text>
    <rect x="210" y="340" width="320" height="14" rx="7" fill="#EEF0FA"/>
    <rect x="210" y="340" width="{cache_w}" height="14" rx="7" fill="#8B5CF6"/>
    <text x="548" y="352" font-size="12" font-weight="700" fill="#151531">{cache_str} cache hits · {rewrites} hook flag rewrites</text>
  </g>

  <!-- Footer -->
  <text x="36" y="414" font-size="11.5" fill="#6B7280">100% local &amp; privacy-sanitized · Zero code, file paths, or prompts included</text>
  <text x="844" y="414" font-size="11.5" font-weight="700" fill="#5B54FA" text-anchor="end">github.com/hatimmakki/efficient-ai</text>
</svg>
"##,
        subtitle = xml_escape(&subtitle),
        agents = xml_escape(&agents_label),
        saved_tok = xml_escape(&saved_tok),
        direct_tok = xml_escape(&direct_tok),
        est_saved = report.est_direct_usd_saved,
        cache_str = cache_str,
        ranged_str = ranged_str,
        prefix_tok = report.current_static_prefix_tokens,
        recovery_str = recovery_str,
        loops = report.local_stats.loops_intercepted,
        out_w = out_w,
        sqz_w = sqz_w,
        cache_w = cache_w,
        out_calls = report.local_stats.outline_calls,
        out_saved = fmt_compact(report.local_stats.outline_tokens_saved as u64),
        sqz_calls = report.local_stats.squeeze_calls,
        sqz_saved = fmt_compact(report.local_stats.squeeze_tokens_saved as u64),
        sqz_lines = report.local_stats.squeeze_lines_elided,
        rewrites = report.local_stats.hook_rewrites,
    )
}

pub fn cmd_share(args: &[String]) -> i32 {
    let mut project = PathBuf::from(".");
    let mut since_days: u64 = 30;
    let mut out_path = PathBuf::from("eai-efficiency-card.svg");

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--project" | "-p" if i + 1 < args.len() => {
                project = PathBuf::from(&args[i + 1]);
                i += 1;
            }
            "--since" if i + 1 < args.len() => {
                since_days = parse_since_days(&args[i + 1]).unwrap_or(30);
                i += 1;
            }
            "--out" | "-o" if i + 1 < args.len() => {
                out_path = PathBuf::from(&args[i + 1]);
                i += 1;
            }
            "-h" | "--help" => {
                println!(
                    "Usage: eai share [--project <dir>] [--since 30d] [--out eai-efficiency-card.svg]\n\
                     Generates a privacy-sanitized SVG Efficiency Proof Card ready for READMEs, PRs, or social sharing."
                );
                return 0;
            }
            _ => {}
        }
        i += 1;
    }

    let (report, _) = build_impact_report(&project, since_days, "auto", Some(250));
    let svg = render_proof_card_svg(&report);
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = fs::create_dir_all(parent);
        }
    }
    match fs::write(&out_path, svg) {
        Ok(()) => {
            println!(
                "Generated privacy-sanitized proof card: {}",
                out_path.display()
            );
            0
        }
        Err(e) => {
            eprintln!("Error writing {}: {}", out_path.display(), e);
            1
        }
    }
}

/// Live terminal session monitor (`eai watch [--project <dir>] [--interval 2] [--once]`).
pub fn cmd_watch(args: &[String]) -> i32 {
    let mut project = PathBuf::from(".");
    let mut interval_secs: u64 = 2;
    let mut once = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--project" | "-p" if i + 1 < args.len() => {
                project = PathBuf::from(&args[i + 1]);
                i += 1;
            }
            "--interval" if i + 1 < args.len() => {
                interval_secs = args[i + 1].parse().unwrap_or(2).max(1);
                i += 1;
            }
            "--once" => once = true,
            "-h" | "--help" => {
                println!(
                    "Usage: eai watch [--project <dir>] [--interval 2] [--once]\n\
                     Streams live active-session tokens, cache-hit alerts, and efficient-ai savings."
                );
                return 0;
            }
            _ => {}
        }
        i += 1;
    }

    let mut prev_turns = 0usize;
    let mut prev_events = 0usize;
    let mut prev_cache_pct: Option<f64> = None;

    println!("Watching active AI session & local `.eai/metrics.jsonl` (Ctrl+C to stop)...");

    loop {
        let events = read_events(&project.join(".eai").join("metrics.jsonl"));
        let stats = summarize_events("project", &events);
        let newest = newest_transcript_file("auto");

        if let Some((path, agent, _mt)) = newest {
            if let Some(sess) = detect_and_parse(&path, &agent) {
                let turns = sess.turn_count();
                let chr = sess.cache_hit_rate() * 100.0;
                if once || turns != prev_turns || events.len() != prev_events {
                    let latest_ctx = sess.turns.last().map(|t| t.total_input()).unwrap_or(0);
                    println!(
                        "[{}] turns={} | ctx={} | cache={:.1}% | est=${:.3} | eai saved={} tok (recovery {:.1}%)",
                        sess.agent,
                        turns,
                        fmt_compact(latest_ctx),
                        chr,
                        sess.estimated_usd(),
                        fmt_compact(stats.total_tokens_saved as u64),
                        stats.recovery_rate_pct
                    );
                    if let Some(prev_c) = prev_cache_pct {
                        if turns > prev_turns && prev_c - chr > 10.0 {
                            println!(
                                "  [CACHE ALERT] Prompt-cache hit rate dropped {:.1}% -> {:.1}%! Check for prefix mutations.",
                                prev_c, chr
                            );
                        }
                    }
                    prev_turns = turns;
                    prev_events = events.len();
                    prev_cache_pct = Some(chr);
                }
            }
        } else if once {
            println!(
                "No active transcripts found yet | local eai saved={} tok",
                fmt_compact(stats.total_tokens_saved as u64)
            );
        }

        if once {
            break;
        }
        thread::sleep(Duration::from_secs(interval_secs));
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proof_card_svg_is_valid_and_sanitized() {
        let report = ImpactReport {
            project_name: "secret-internal-repo".into(),
            window_days: 14,
            has_baseline: true,
            baseline: None,
            current_sessions: 42,
            current_turns: 1200,
            current_avg_usd_per_session: 0.85,
            current_avg_cw_per_session: 280000.0,
            current_cache_hit_pct: 97.4,
            current_whole_read_pct: 22.0,
            current_static_prefix_tokens: 240,
            total_spend_usd: 35.7,
            est_direct_usd_saved: 12.4,
            local_stats: StatsSummary {
                scope: "secret-internal-repo".into(),
                total_events: 50,
                outline_calls: 30,
                outline_tokens_saved: 48000,
                squeeze_calls: 20,
                squeeze_tokens_saved: 22000,
                squeeze_lines_elided: 1840,
                squeeze_spilled_logs: 18,
                hook_rewrites: 45,
                loops_intercepted: 3,
                recoveries: 0,
                recovery_rate_pct: 0.0,
                total_tokens_saved: 70000,
                estimated_resent_tokens_avoided: 1750000,
            },
            comparisons: vec![],
            advisor_tips: vec![],
            agents_active: vec!["claude".into(), "antigravity".into()],
        };
        let svg = render_proof_card_svg(&report);
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("97.4%"));
        assert!(svg.contains("1.8M"));
        assert!(svg.contains("Claude Code"));
        assert!(svg.contains("Antigravity"));
        // Privacy guarantee: repo name is NEVER embedded in the shareable SVG card!
        assert!(!svg.contains("secret-internal-repo"));
    }
}
