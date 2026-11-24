//! Local privacy-first AI Efficiency & Impact Dashboard (`eai dashboard`).
//!
//! Serves a self-contained single-binary web UI on `http://127.0.0.1:<port>` with zero
//! external CDN calls, zero cloud telemetry, and live auto-refresh from local transcripts
//! and `.eai/metrics.jsonl`.

use crate::impact::{build_impact_report, render_proof_card_svg};
use crate::telemetry::read_events;
use crate::tokscope::audit_directory;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn build_dashboard_payload(
    project_dir: &Path,
    since_days: u64,
    agent: &str,
    scope_project: bool,
) -> Value {
    let (report, summary) =
        build_impact_report(project_dir, since_days, agent, Some(300), scope_project);
    let audit_items = audit_directory(project_dir, false);
    let recent_events = read_events(&project_dir.join(".eai").join("metrics.jsonl"));
    let recent_tail: Vec<_> = recent_events.into_iter().rev().take(25).collect();

    let mut top_projects: Vec<Value> = summary
        .by_project
        .iter()
        .map(|(name, pb)| {
            let chr = if pb.total_input > 0 {
                100.0 * (pb.cache_read as f64) / (pb.total_input as f64)
            } else {
                0.0
            };
            json!({
                "project": name,
                "sessions": pb.sessions,
                "turns": pb.turns,
                "cache_hit_pct": (chr * 10.0).round() / 10.0,
                "estimated_usd": (pb.estimated_usd * 100.0).round() / 100.0,
            })
        })
        .collect();
    top_projects.sort_by(|a, b| {
        b["estimated_usd"]
            .as_f64()
            .unwrap_or(0.0)
            .partial_cmp(&a["estimated_usd"].as_f64().unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    top_projects.truncate(8);

    let mut daily: Vec<_> = summary.by_day.values().collect();
    if daily.len() > 30 {
        daily = daily[daily.len() - 30..].to_vec();
    }

    let mut top_cats: Vec<Value> = summary
        .first_tok_by_cat
        .iter()
        .map(|(cat, &first_tok)| {
            let calls = *summary.calls_by_cat.get(cat).unwrap_or(&0);
            let resent = *summary.resent_tok_by_cat.get(cat).unwrap_or(&0);
            json!({
                "category": cat,
                "calls": calls,
                "first_tokens": first_tok,
                "resent_tokens": resent,
            })
        })
        .collect();
    top_cats.sort_by(|a, b| {
        b["resent_tokens"]
            .as_u64()
            .unwrap_or(0)
            .cmp(&a["resent_tokens"].as_u64().unwrap_or(0))
    });
    top_cats.truncate(8);

    let mut models: Vec<Value> = summary
        .by_model
        .iter()
        .map(|(m, mb)| {
            json!({
                "model": m,
                "turns": mb.turns,
                "raw_tokens": mb.raw_tokens,
                "estimated_usd": (mb.estimated_usd * 100.0).round() / 100.0,
            })
        })
        .collect();
    models.sort_by(|a, b| {
        b["estimated_usd"]
            .as_f64()
            .unwrap_or(0.0)
            .partial_cmp(&a["estimated_usd"].as_f64().unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    models.truncate(8);

    json!({
        "report": report,
        "by_agent": summary.by_agent,
        "top_projects": top_projects,
        "daily": daily,
        "top_categories": top_cats,
        "top_models": models,
        "audit_items": audit_items,
        "recent_events": recent_tail,
    })
}

fn parse_query_param(target: &str, key: &str) -> Option<String> {
    let (_, q) = target.split_once('?')?;
    for pair in q.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k == key {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn write_http_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) {
    let header = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        status,
        content_type,
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

fn handle_client(mut stream: TcpStream, project_dir: &Path, default_days: u64) {
    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });
    let mut req_line = String::new();
    if reader.read_line(&mut req_line).is_err() {
        return;
    }
    let target = req_line.split_whitespace().nth(1).unwrap_or("/");

    if target == "/" || target.starts_with("/?") {
        write_http_response(&mut stream, "200 OK", "text/html; charset=utf-8", DASHBOARD_HTML.as_bytes());
    } else if target.starts_with("/api/overview") {
        let days = parse_query_param(target, "since")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(default_days);
        let agent = parse_query_param(target, "agent").unwrap_or_else(|| "auto".to_string());
        let scope_project = parse_query_param(target, "scope").as_deref() == Some("project");
        let payload = build_dashboard_payload(project_dir, days, &agent, scope_project);
        let body = serde_json::to_vec(&payload).unwrap_or_else(|_| b"{}".to_vec());
        write_http_response(&mut stream, "200 OK", "application/json; charset=utf-8", &body);
    } else if target.starts_with("/api/card.svg") {
        let days = parse_query_param(target, "since")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(default_days);
        let scope_project = parse_query_param(target, "scope").as_deref() == Some("project");
        let (report, _) = build_impact_report(project_dir, days, "auto", Some(300), scope_project);
        let svg = render_proof_card_svg(&report);
        write_http_response(&mut stream, "200 OK", "image/svg+xml; charset=utf-8", svg.as_bytes());
    } else {
        write_http_response(&mut stream, "404 Not Found", "text/plain; charset=utf-8", b"Not Found");
    }
}

fn open_browser(url: &str) {
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("cmd").args(["/C", "start", "", url]).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg(url).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("xdg-open").arg(url).spawn();
    }
}

pub fn cmd_dashboard(args: &[String]) -> i32 {
    let mut port: u16 = 7420;
    let mut project = PathBuf::from(".");
    let mut since_days: u64 = 30;
    let mut no_open = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--port" if i + 1 < args.len() => {
                port = args[i + 1].parse().unwrap_or(7420);
                i += 1;
            }
            "--project" | "-p" if i + 1 < args.len() => {
                project = PathBuf::from(&args[i + 1]);
                i += 1;
            }
            "--since" if i + 1 < args.len() => {
                since_days = crate::tokscope::parse_since_days(&args[i + 1]).unwrap_or(30);
                i += 1;
            }
            "--no-open" => no_open = true,
            "-h" | "--help" => {
                println!(
                    "Usage: eai dashboard [--port 7420] [--project <dir>] [--since 30d] [--no-open]\n\
                     Launch the 100% local AI Performance & Impact Dashboard on http://127.0.0.1:<port>."
                );
                return 0;
            }
            _ => {}
        }
        i += 1;
    }

    let addr = format!("127.0.0.1:{}", port);
    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Error binding to {}: {}", addr, e);
            return 1;
        }
    };

    let url = format!("http://{}", addr);
    println!("efficient-ai Local Dashboard running at {}", url);
    println!("100% offline/private — press Ctrl+C to stop.");
    if !no_open {
        open_browser(&url);
    }

    for stream in listener.incoming().flatten() {
        handle_client(stream, &project, since_days);
    }
    0
}

const DASHBOARD_HTML: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>efficient-ai · Local AI Performance & Impact Monitor</title>
<style>
  :root {
    --bg: #F6F7FB;
    --card: #FFFFFF;
    --ink: #151531;
    --muted: #5E6478;
    --border: #E4E7F2;
    --accent: #5B54FA;
    --accent-soft: #EEF0FF;
    --green: #10B981;
    --green-soft: #ECFDF5;
    --red: #F43F5E;
    --red-soft: #FFF1F2;
    --amber: #D97706;
    --amber-soft: #FFFBEB;
  }
  * { box-sizing: border-box; }
  body {
    margin: 0;
    font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    background: radial-gradient(circle at 50% -10%, rgba(98,93,245,0.14), transparent 45%), var(--bg);
    color: var(--ink);
    line-height: 1.45;
  }
  .wrap { max-width: 1180px; margin: 0 auto; padding: 28px 24px 56px; }
  header {
    display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between;
    gap: 16px; margin-bottom: 24px;
  }
  .brand { display: flex; align-items: center; gap: 12px; }
  .brand-bar { width: 10px; height: 38px; border-radius: 6px; background: linear-gradient(180deg,#5B54FA,#8B5CF6); }
  .brand h1 { margin: 0; font-size: 22px; font-weight: 800; letter-spacing: -0.02em; }
  .brand p { margin: 2px 0 0; font-size: 13px; color: var(--muted); }
  .controls { display: flex; flex-wrap: wrap; align-items: center; gap: 10px; }
  select, .btn {
    border: 1px solid var(--border); background: var(--card); color: var(--ink);
    padding: 8px 12px; border-radius: 10px; font-size: 13px; font-weight: 600; cursor: pointer;
    text-decoration: none;
  }
  .btn-primary { background: var(--accent); color: #fff; border-color: var(--accent); }
  .live-pill {
    display: inline-flex; align-items: center; gap: 6px; font-size: 12px; font-weight: 600;
    color: var(--green); background: var(--green-soft); padding: 6px 10px; border-radius: 99px;
  }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--green); animation: pulse 2s infinite; }
  @keyframes pulse { 0%,100% { opacity: 1; } 50% { opacity: 0.35; } }
  .grid-4 { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 16px; margin-bottom: 20px; }
  .grid-2 { display: grid; grid-template-columns: repeat(auto-fit, minmax(460px, 1fr)); gap: 18px; margin-bottom: 20px; }
  .card {
    background: var(--card); border: 1px solid var(--border); border-radius: 16px; padding: 20px;
    box-shadow: 0 6px 18px rgba(21,21,49,0.04);
  }
  .kpi-label { font-size: 11px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); }
  .kpi-val { font-size: 30px; font-weight: 800; margin: 8px 0 4px; font-variant-numeric: tabular-nums; }
  .kpi-sub { font-size: 12.5px; color: var(--muted); }
  .accent { color: var(--accent); }
  .good { color: var(--green); }
  .bad { color: var(--red); }
  h2 { margin: 0 0 14px; font-size: 15px; font-weight: 700; display: flex; justify-content: space-between; align-items: center; }
  table { width: 100%; border-collapse: collapse; font-size: 13px; }
  th { text-align: left; font-size: 11px; text-transform: uppercase; letter-spacing: 0.05em; color: var(--muted); padding: 8px 6px; border-bottom: 1px solid var(--border); }
  td { padding: 10px 6px; border-bottom: 1px solid #F1F3F9; font-variant-numeric: tabular-nums; }
  td.num, th.num { text-align: right; }
  .pill { display: inline-block; padding: 3px 8px; border-radius: 99px; font-size: 11.5px; font-weight: 700; }
  .pill-good { background: var(--green-soft); color: var(--green); }
  .pill-bad { background: var(--red-soft); color: var(--red); }
  .pill-warn { background: var(--amber-soft); color: var(--amber); }
  .bar-row { margin-bottom: 12px; }
  .bar-meta { display: flex; justify-content: space-between; font-size: 12.5px; margin-bottom: 4px; }
  .bar-track { height: 10px; background: #EEF0FA; border-radius: 99px; overflow: hidden; }
  .bar-fill { height: 100%; border-radius: 99px; background: linear-gradient(90deg,#5B54FA,#8B5CF6); }
  .chart { display: flex; align-items: flex-end; gap: 6px; height: 150px; padding-top: 12px; }
  .chart-col { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: flex-end; height: 100%; min-width: 14px; }
  .chart-bar { width: 100%; border-radius: 6px 6px 2px 2px; background: linear-gradient(180deg,#5B54FA,#8B5CF6); min-height: 4px; transition: height 0.25s; }
  .chart-lbl { font-size: 10px; color: var(--muted); margin-top: 6px; white-space: nowrap; }
  .tip { padding: 11px 13px; border-radius: 12px; margin-bottom: 10px; font-size: 13px; border: 1px solid var(--border); }
  .tip-high { background: var(--red-soft); border-color: #FECDD3; }
  .tip-medium { background: var(--amber-soft); border-color: #FDE68A; }
  .tip-good { background: var(--green-soft); border-color: #A7F3D0; }
  .tip strong { display: block; margin-bottom: 2px; }
  .empty { color: var(--muted); font-size: 13px; padding: 10px 0; }
</style>
</head>
<body>
<div class="wrap">
  <header>
    <div class="brand">
      <div class="brand-bar"></div>
      <div>
        <h1>efficient-ai <span style="font-weight:500;color:var(--muted)" id="proj-name"></span></h1>
        <p>100% Local AI Performance Monitor &amp; Before/After Impact Proof</p>
      </div>
    </div>
    <div class="controls">
      <span class="live-pill"><span class="dot"></span> Local Live</span>
      <select id="scope">
        <option value="project" selected>This Repo</option>
        <option value="all">All Projects</option>
      </select>
      <select id="since">
        <option value="7">Last 7 days</option>
        <option value="14">Last 14 days</option>
        <option value="30" selected>Last 30 days</option>
        <option value="90">Last 90 days</option>
      </select>
      <select id="agent">
        <option value="auto" selected>All Agents</option>
        <option value="claude">Claude Code</option>
        <option value="antigravity">Antigravity</option>
        <option value="gemini">Gemini CLI</option>
        <option value="codex">Codex CLI</option>
      </select>
      <a class="btn btn-primary" id="card-link" href="/api/card.svg?since=30" download="eai-efficiency-card.svg">Download Proof Card (SVG)</a>
    </div>
  </header>

  <section class="grid-4" id="kpis"></section>

  <section class="grid-2">
    <div class="card">
      <h2>Before vs. After <code>efficient-ai</code> <span id="baseline-tag"></span></h2>
      <div id="before-after"></div>
    </div>
    <div class="card">
      <h2>Daily Spend Trend (Est. USD) <span style="font-size:12px;color:var(--muted)" id="daily-total"></span></h2>
      <div class="chart" id="daily-chart"></div>
    </div>
  </section>

  <section class="grid-2">
    <div class="card">
      <h2>Breakdown by Agent</h2>
      <div id="agents-table"></div>
      <h2 style="margin-top:20px">Top Projects by Spend</h2>
      <div id="projects-table"></div>
    </div>
    <div class="card">
      <h2>Tool-Output Exposure (Re-sent History)</h2>
      <div id="cats-bars"></div>
    </div>
  </section>

  <section class="grid-2">
    <div class="card">
      <h2>Optimization Advisor</h2>
      <div id="advisor"></div>
    </div>
    <div class="card">
      <h2>Live Local Telemetry (<code>.eai/metrics.jsonl</code>)</h2>
      <div id="events-table"></div>
    </div>
  </section>
</div>

<script>
const fmt = (n) => n >= 1e9 ? (n/1e9).toFixed(2)+'B' : n >= 1e6 ? (n/1e6).toFixed(1)+'M' : n >= 1e3 ? (n/1e3).toFixed(1)+'k' : String(Math.round(n));
const esc = (s) => String(s).replace(/[&<>"]/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c]));

async function refresh() {
  const scope = document.getElementById('scope').value;
  const since = document.getElementById('since').value;
  const agent = document.getElementById('agent').value;
  document.getElementById('card-link').href = `/api/card.svg?since=${since}&scope=${scope}`;
  const res = await fetch(`/api/overview?since=${since}&agent=${agent}&scope=${scope}`);
  const data = await res.json();
  const r = data.report;
  const st = r.local_stats;

  document.getElementById('proj-name').textContent = '· ' + r.project_name + (r.scope_label === 'project' ? ' (repo-scoped)' : ' (all projects)');

  document.getElementById('kpis').innerHTML = `
    <div class="card">
      <div class="kpi-label">Est. Spend (${r.window_days}d)</div>
      <div class="kpi-val">$${r.total_spend_usd.toFixed(2)}</div>
      <div class="kpi-sub">${r.current_sessions} sessions · ${fmt(r.current_turns)} turns ($${r.current_avg_usd_per_session.toFixed(2)}/sess)</div>
    </div>
    <div class="card">
      <div class="kpi-label">Tokens Saved (Local)</div>
      <div class="kpi-val accent">${fmt(st.estimated_resent_tokens_avoided)}</div>
      <div class="kpi-sub">${fmt(st.total_tokens_saved)} direct (${st.outline_calls} outlines, ${st.squeeze_calls} squeezes) · ~$${r.est_direct_usd_saved.toFixed(2)}</div>
    </div>
    <div class="card">
      <div class="kpi-label">Prompt-Cache Hit Rate</div>
      <div class="kpi-val ${r.current_cache_hit_pct >= 90 ? 'good' : 'bad'}">${r.current_cache_hit_pct.toFixed(1)}%</div>
      <div class="kpi-sub">Whole-file Read: ${r.current_whole_read_pct.toFixed(1)}% · Prefix: ${r.current_static_prefix_tokens} tok</div>
    </div>
    <div class="card">
      <div class="kpi-label">Quality Guardrail (Recovery)</div>
      <div class="kpi-val ${st.recovery_rate_pct <= 5 ? 'good' : 'bad'}">${st.recovery_rate_pct.toFixed(1)}%</div>
      <div class="kpi-sub">${st.recoveries}/${st.squeeze_spilled_logs} logs opened · ${st.loops_intercepted} stuck loops blocked</div>
    </div>`;

  // Before/After
  const ba = document.getElementById('before-after');
  document.getElementById('baseline-tag').innerHTML = r.has_baseline
    ? '<span class="pill pill-good">Baseline Active</span>'
    : '<span class="pill pill-warn">Run eai init</span>';
  if (r.comparisons && r.comparisons.length) {
    ba.innerHTML = `<table><thead><tr><th>Metric</th><th class="num">Before</th><th class="num">With EAI</th><th class="num">Delta</th></tr></thead><tbody>` +
      r.comparisons.map(c => `<tr><td>${esc(c.metric)}</td><td class="num">${esc(c.before)}</td><td class="num">${esc(c.after)}</td><td class="num"><span class="pill ${c.improved ? 'pill-good':'pill-bad'}">${esc(c.delta)}</span></td></tr>`).join('') +
      `</tbody></table>`;
  } else {
    ba.innerHTML = `<div class="empty">No <code>.eai/baseline.json</code> captured yet. Run <code>eai init</code> in this repo to unlock automatic Before vs. After tracking.</div>`;
  }

  // Daily chart
  const daily = data.daily || [];
  const maxUsd = Math.max(...daily.map(d => d.estimated_usd), 0.01);
  document.getElementById('daily-total').textContent = `${daily.length} active days`;
  const dc = document.getElementById('daily-chart');
  if (!daily.length) {
    dc.innerHTML = '<div class="empty">No session timestamps in selected window.</div>';
  } else {
    dc.innerHTML = daily.slice(-14).map(d => {
      const h = Math.max(6, Math.round((d.estimated_usd / maxUsd) * 118));
      return `<div class="chart-col" title="${d.date}: $${d.estimated_usd.toFixed(2)} (${d.sessions} sessions, ${d.turns} turns)">
        <div class="chart-bar" style="height:${h}px"></div>
        <div class="chart-lbl">${d.date.slice(5)}</div>
      </div>`;
    }).join('');
  }

  // Agents table
  const agents = Object.entries(data.by_agent || {});
  document.getElementById('agents-table').innerHTML = agents.length
    ? `<table><thead><tr><th>Agent</th><th class="num">Sessions</th><th class="num">Turns</th><th class="num">Cache %</th><th class="num">Est. USD</th></tr></thead><tbody>` +
      agents.map(([name, a]) => {
        const chr = a.total_input > 0 ? (100 * a.cache_read / a.total_input).toFixed(1) : '0.0';
        return `<tr><td><strong>${esc(name)}</strong></td><td class="num">${a.sessions}</td><td class="num">${fmt(a.turns)}</td><td class="num">${chr}%</td><td class="num"><strong>$${a.estimated_usd.toFixed(2)}</strong></td></tr>`;
      }).join('') + `</tbody></table>`
    : `<div class="empty">No transcripts found for the selected filter.</div>`;

  const projs = data.top_projects || [];
  document.getElementById('projects-table').innerHTML = projs.length
    ? `<table><thead><tr><th>Project</th><th class="num">Sessions</th><th class="num">Cache %</th><th class="num">Est. USD</th></tr></thead><tbody>` +
      projs.map(p => `<tr><td><strong>${esc(p.project)}</strong></td><td class="num">${p.sessions}</td><td class="num">${p.cache_hit_pct.toFixed(1)}%</td><td class="num"><strong>$${p.estimated_usd.toFixed(2)}</strong></td></tr>`).join('') + `</tbody></table>`
    : `<div class="empty">No project paths detected yet.</div>`;

  // Category bars
  const cats = data.top_categories || [];
  const totResent = cats.reduce((acc, c) => acc + c.resent_tokens, 0) || 1;
  document.getElementById('cats-bars').innerHTML = cats.length
    ? cats.map(c => {
        const pct = (100 * c.resent_tokens / totResent);
        return `<div class="bar-row"><div class="bar-meta"><span>${esc(c.category)} (${c.calls} calls)</span><strong>${fmt(c.resent_tokens)} (${pct.toFixed(1)}%)</strong></div><div class="bar-track"><div class="bar-fill" style="width:${Math.max(2,pct)}%"></div></div></div>`;
      }).join('')
    : `<div class="empty">No tool observations in selected window.</div>`;

  // Advisor
  const tips = r.advisor_tips || [];
  document.getElementById('advisor').innerHTML = tips.length
    ? tips.map(t => `<div class="tip tip-${esc(t.severity)}"><strong>${esc(t.title)}</strong>${esc(t.detail)}</div>`).join('')
    : `<div class="empty">Everything looks healthy!</div>`;

  // Live events
  const evs = data.recent_events || [];
  document.getElementById('events-table').innerHTML = evs.length
    ? `<table><thead><tr><th>Tool</th><th>Action</th><th class="num">Saved Tok</th><th class="num">Elided</th></tr></thead><tbody>` +
      evs.slice(0, 10).map(e => `<tr><td><strong>${esc(e.tool)}</strong></td><td>${esc(e.action)}</td><td class="num">${fmt(e.saved_tokens_est)}</td><td class="num">${e.elided_lines}</td></tr>`).join('') +
      `</tbody></table>`
    : `<div class="empty">No local events yet. Use <code>eai outline</code>, <code>eai squeeze</code>, or active hooks to stream live savings here.</div>`;
}

document.getElementById('scope').addEventListener('change', refresh);
document.getElementById('since').addEventListener('change', refresh);
document.getElementById('agent').addEventListener('change', refresh);
refresh();
setInterval(refresh, 5000);
</script>
</body>
</html>
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dashboard_payload_and_embedded_html() {
        let cwd = std::env::current_dir().unwrap();
        let payload = build_dashboard_payload(&cwd, 7, "auto", true);
        assert!(payload["report"].is_object());
        assert!(payload["top_projects"].is_array());
        assert!(payload["daily"].is_array());
        assert!(DASHBOARD_HTML.contains("/api/overview"));
        assert!(DASHBOARD_HTML.contains("/api/card.svg"));
    }
}
