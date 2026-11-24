//! Opt-in local Cache-Aware Batch Observation Masking proxy (`eai proxy`) & simulator.
//!
//! Productizes Experiment 007:
//! - Naive per-turn sliding-window masking busts prompt caches on every turn (+9% to +112% Claude cost).
//! - Cache-Aware Batch Masking masks tool observations older than `M` turns ONLY at fixed `K`-turn
//!   batch boundaries so already-masked prefix blocks stay 100% byte-identical across compactions.
//! - Also detects mid-session static-prefix mutations (`[CACHE-BUST ALERT]`) and logs savings to
//!   `.eai/metrics.jsonl`.

use crate::telemetry::{append_event, est_tokens_from_chars, now_unix_secs, MetricEvent};
use crate::tokscope::{detect_and_parse, weights_for_agent};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Debug, Clone, Copy)]
pub struct BatchMaskConfig {
    pub batch_k: usize,
    pub keep_m: usize,
    pub min_chars: usize,
}

impl BatchMaskConfig {
    pub fn for_provider(provider: &str) -> Self {
        match provider {
            "openai" | "codex" => Self {
                batch_k: 8,
                keep_m: 6,
                min_chars: 480,
            },
            _ => Self {
                batch_k: 12,
                keep_m: 6,
                min_chars: 480,
            },
        }
    }
}

#[derive(Debug, Default)]
pub struct MaskOutcome {
    pub total_observations: usize,
    pub masked_count: usize,
    pub chars_saved: usize,
    pub active_boundary: usize,
    pub cache_bust_detected: bool,
}

#[derive(Debug, Default)]
pub struct PrefixTracker {
    /// Maps conversation fingerprint (first user message hash) -> last static prefix hash
    by_conv: HashMap<u64, u64>,
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn compute_conv_and_prefix_hashes(body: &Value) -> (u64, u64) {
    let mut prefix_buf = String::new();
    if let Some(m) = body.get("model") {
        prefix_buf.push_str(&m.to_string());
    }
    if let Some(sys) = body.get("system") {
        prefix_buf.push_str(&sys.to_string());
    }
    if let Some(tools) = body.get("tools") {
        prefix_buf.push_str(&tools.to_string());
    }
    let prefix_hash = fnv1a_64(prefix_buf.as_bytes());

    let first_user = body
        .get("messages")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
        })
        .map(|m| m.to_string())
        .unwrap_or_default();
    let conv_hash = fnv1a_64(first_user.as_bytes());
    (conv_hash, prefix_hash)
}

fn deterministic_placeholder(orig_chars: usize, obs_idx: usize, batch_k: usize) -> String {
    // Depends ONLY on (orig_chars, obs_idx, batch_k) — NEVER on the current turn —
    // guaranteeing 100% byte-identical prefix stability across all subsequent compactions.
    let batch_id = (obs_idx / batch_k.max(1)) + 1;
    format!(
        "[eai masked: {} chars (~{} tok) from obs #{}; batch B{} frozen for prompt-cache stability]",
        orig_chars,
        est_tokens_from_chars(orig_chars),
        obs_idx + 1,
        batch_id
    )
}

fn value_text_len(v: &Value) -> usize {
    match v {
        Value::String(s) => s.len(),
        Value::Array(arr) => arr.iter().map(value_text_len).sum(),
        Value::Object(map) => {
            if let Some(Value::String(t)) = map.get("text") {
                t.len()
            } else {
                v.to_string().len()
            }
        }
        _ => 0,
    }
}

/// Apply prefix-stable Cache-Aware Batch Observation Masking to an Anthropic Messages or
/// OpenAI Chat Completions JSON request body in place.
pub fn apply_batch_masking(
    body: &mut Value,
    cfg: BatchMaskConfig,
    tracker: Option<&mut PrefixTracker>,
) -> MaskOutcome {
    let mut outcome = MaskOutcome::default();

    // 1. Check static prefix stability (cache-bust detector)
    if let Some(tr) = tracker {
        let (conv_h, pref_h) = compute_conv_and_prefix_hashes(body);
        if let Some(&prev) = tr.by_conv.get(&conv_h) {
            if prev != pref_h {
                outcome.cache_bust_detected = true;
            }
        }
        tr.by_conv.insert(conv_h, pref_h);
    }

    let Some(messages) = body.get_mut("messages").and_then(|v| v.as_array_mut()) else {
        return outcome;
    };

    // 2. Collect all tool-observation locations across Anthropic (`tool_result` blocks) and OpenAI (`role == "tool"`)
    #[derive(Clone, Copy)]
    enum ObsLoc {
        AnthropicBlock(usize, usize), // (msg_idx, block_idx)
        OpenAiToolMsg(usize),         // msg_idx
    }

    let mut locs: Vec<ObsLoc> = Vec::new();
    for (mi, msg) in messages.iter().enumerate() {
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
        if role == "tool" {
            locs.push(ObsLoc::OpenAiToolMsg(mi));
        } else if role == "user" {
            if let Some(blocks) = msg.get("content").and_then(|v| v.as_array()) {
                for (bi, blk) in blocks.iter().enumerate() {
                    if blk.get("type").and_then(|v| v.as_str()) == Some("tool_result") {
                        locs.push(ObsLoc::AnthropicBlock(mi, bi));
                    }
                }
            }
        }
    }

    outcome.total_observations = locs.len();
    let eligible = locs.len().saturating_sub(cfg.keep_m);
    let active_boundary = (eligible / cfg.batch_k.max(1)) * cfg.batch_k.max(1);
    outcome.active_boundary = active_boundary;

    if active_boundary == 0 {
        return outcome;
    }

    // 3. Deterministically mask every eligible observation < active_boundary exceeding `min_chars`
    for (obs_idx, loc) in locs.iter().take(active_boundary).enumerate() {
        match *loc {
            ObsLoc::AnthropicBlock(mi, bi) => {
                if let Some(blk) = messages
                    .get_mut(mi)
                    .and_then(|m| m.get_mut("content"))
                    .and_then(|c| c.as_array_mut())
                    .and_then(|arr| arr.get_mut(bi))
                    .and_then(|b| b.as_object_mut())
                {
                    let orig_len = blk.get("content").map(value_text_len).unwrap_or(0);
                    if orig_len >= cfg.min_chars {
                        let placeholder = deterministic_placeholder(orig_len, obs_idx, cfg.batch_k);
                        let saved = orig_len.saturating_sub(placeholder.len());
                        blk.insert("content".to_string(), Value::String(placeholder));
                        outcome.masked_count += 1;
                        outcome.chars_saved += saved;
                    }
                }
            }
            ObsLoc::OpenAiToolMsg(mi) => {
                if let Some(obj) = messages.get_mut(mi).and_then(|m| m.as_object_mut()) {
                    let orig_len = obj.get("content").map(value_text_len).unwrap_or(0);
                    if orig_len >= cfg.min_chars {
                        let placeholder = deterministic_placeholder(orig_len, obs_idx, cfg.batch_k);
                        let saved = orig_len.saturating_sub(placeholder.len());
                        obj.insert("content".to_string(), Value::String(placeholder));
                        outcome.masked_count += 1;
                        outcome.chars_saved += saved;
                    }
                }
            }
        }
    }

    outcome
}

// =========================================================================
// Dry-Run Transcript Simulator (`eai proxy --simulate <transcript>`)
// =========================================================================

#[derive(Debug, Clone)]
pub struct SimRow {
    pub strategy: String,
    pub raw_input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub cost_weighted: f64,
}

/// Replay a sequence of per-turn observation token sizes under exact prefix-stability caching.
pub fn simulate_masking_trajectory(
    static_prefix: u64,
    turn_obs_tokens: &[u64],
    agent: &str,
    batch_k: usize,
    keep_m: usize,
    min_tok: u64,
) -> Vec<SimRow> {
    let w = weights_for_agent(agent);
    let placeholder_tok: u64 = 24;

    let run_one = |label: &str, mode: &str| -> SimRow {
        let mut prev_tokens: Vec<u64> = Vec::new();
        let mut raw_input = 0u64;
        let mut cache_read = 0u64;
        let mut cache_write = 0u64;

        for t in 0..turn_obs_tokens.len() {
            let mut cur: Vec<u64> = Vec::with_capacity(t + 2);
            cur.push(static_prefix);

            let mask_limit = match mode {
                "baseline" => 0,
                "naive" => (t + 1).saturating_sub(keep_m),
                "batch" => {
                    let elig = (t + 1).saturating_sub(keep_m);
                    (elig / batch_k.max(1)) * batch_k.max(1)
                }
                _ => 0,
            };

            for (i, &obs_tok) in turn_obs_tokens.iter().take(t + 1).enumerate() {
                if i < mask_limit && obs_tok >= min_tok {
                    cur.push(placeholder_tok);
                } else {
                    cur.push(obs_tok);
                }
            }

            // Longest common byte-identical prefix between prev_tokens and cur
            let mut lcp_tok = 0u64;
            for (a, b) in prev_tokens.iter().zip(cur.iter()) {
                if a == b {
                    lcp_tok += *a;
                } else {
                    break;
                }
            }
            let total_t: u64 = cur.iter().sum();
            let new_t = total_t.saturating_sub(lcp_tok);

            raw_input += total_t;
            cache_read += lcp_tok;
            cache_write += new_t;
            prev_tokens = cur;
        }

        let cw = (cache_read as f64) * w.cache_read + (cache_write as f64) * w.cache_write_5m;
        SimRow {
            strategy: label.to_string(),
            raw_input,
            cache_read,
            cache_write,
            cost_weighted: cw,
        }
    };

    vec![
        run_one("1. Baseline (No Masking)", "baseline"),
        run_one(
            &format!("2. Naive Per-Turn Sliding (M={})", keep_m),
            "naive",
        ),
        run_one(
            &format!("3. eai proxy Batch Masking (K={}, M={})", batch_k, keep_m),
            "batch",
        ),
    ]
}

fn run_simulate(path: &Path, cfg: BatchMaskConfig) -> i32 {
    let Some(sess) = detect_and_parse(path, "auto") else {
        eprintln!("Error: could not parse transcript: {}", path.display());
        return 1;
    };
    if sess.observations.is_empty() {
        println!("Transcript has 0 tool observations — nothing to simulate.");
        return 0;
    }

    let static_prefix = sess
        .turns
        .first()
        .map(|t| t.total_input().clamp(1500, 15000))
        .unwrap_or(4000);
    let obs_seq: Vec<u64> = sess.observations.iter().map(|o| o.tokens).collect();
    let rows = simulate_masking_trajectory(
        static_prefix,
        &obs_seq,
        &sess.agent,
        cfg.batch_k,
        cfg.keep_m,
        est_tokens_from_chars(cfg.min_chars) as u64,
    );

    let base_raw = rows[0].raw_input.max(1) as f64;
    let base_cw = rows[0].cost_weighted.max(1.0);

    println!(
        "# eai proxy — Cache-Aware Batch Masking Simulation ({}, {} tool observations)\n",
        sess.agent,
        obs_seq.len()
    );
    println!("| Strategy | Raw Input Tok | Δ Raw | Cache Hit % | Cache Writes | Cost-Weighted Tok | Δ Billed Cost |");
    println!("|---|---:|---:|---:|---:|---:|---:|");
    for r in &rows {
        let chr = if r.raw_input > 0 {
            100.0 * (r.cache_read as f64) / (r.raw_input as f64)
        } else {
            0.0
        };
        let d_raw = 100.0 * ((r.raw_input as f64) - base_raw) / base_raw;
        let d_cw = 100.0 * (r.cost_weighted - base_cw) / base_cw;
        println!(
            "| {} | {} | {:+.1}% | {:.1}% | {} | {:.0} | **{:+.1}%** |",
            r.strategy, r.raw_input, d_raw, chr, r.cache_write, r.cost_weighted, d_cw
        );
    }
    0
}

// =========================================================================
// Live HTTP Reverse Proxy (`127.0.0.1:<port>`)
// =========================================================================

fn read_chunked_body<R: BufRead>(reader: &mut R) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let mut size_line = String::new();
        reader.read_line(&mut size_line)?;
        let hex = size_line.trim().split(';').next().unwrap_or("0");
        let sz = usize::from_str_radix(hex, 16).unwrap_or(0);
        if sz == 0 {
            let mut trailer = String::new();
            let _ = reader.read_line(&mut trailer);
            break;
        }
        let mut chunk = vec![0u8; sz];
        reader.read_exact(&mut chunk)?;
        out.extend_from_slice(&chunk);
        let mut crlf = [0u8; 2];
        let _ = reader.read_exact(&mut crlf);
    }
    Ok(out)
}

fn handle_proxy_connection(
    mut client: TcpStream,
    upstream_base: String,
    cfg: BatchMaskConfig,
    tracker: Arc<Mutex<PrefixTracker>>,
    cwd: PathBuf,
    agent: Arc<ureq::Agent>,
) {
    let mut reader = BufReader::new(match client.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });

    let mut req_line = String::new();
    if reader.read_line(&mut req_line).is_err() || req_line.trim().is_empty() {
        return;
    }
    let mut parts = req_line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_string();
    let target = parts.next().unwrap_or("/").to_string();

    let mut headers: Vec<(String, String)> = Vec::new();
    let mut content_len: usize = 0;
    let mut is_chunked = false;

    loop {
        let mut hline = String::new();
        if reader.read_line(&mut hline).is_err() || hline == "\r\n" || hline == "\n" || hline.is_empty() {
            break;
        }
        if let Some((k, v)) = hline.split_once(':') {
            let key = k.trim().to_string();
            let val = v.trim().to_string();
            if key.eq_ignore_ascii_case("content-length") {
                content_len = val.parse().unwrap_or(0);
            } else if key.eq_ignore_ascii_case("transfer-encoding")
                && val.to_ascii_lowercase().contains("chunked")
            {
                is_chunked = true;
            }
            headers.push((key, val));
        }
    }

    let mut body_bytes = if is_chunked {
        read_chunked_body(&mut reader).unwrap_or_default()
    } else if content_len > 0 {
        let mut buf = vec![0u8; content_len];
        if reader.read_exact(&mut buf).is_err() {
            return;
        }
        buf
    } else {
        Vec::new()
    };

    // Apply Cache-Aware Batch Masking on JSON POST requests to chat/messages endpoints
    let is_llm_endpoint = method.eq_ignore_ascii_case("POST")
        && (target.contains("/messages") || target.contains("/chat/completions"));
    if is_llm_endpoint && !body_bytes.is_empty() {
        if let Ok(mut json_body) = serde_json::from_slice::<Value>(&body_bytes) {
            let outcome = {
                let mut tr = tracker.lock().unwrap();
                apply_batch_masking(&mut json_body, cfg, Some(&mut tr))
            };
            if outcome.cache_bust_detected {
                eprintln!("[eai proxy] [CACHE-BUST ALERT] Static prefix (system/tools/model) mutated mid-session!");
                append_event(
                    &cwd,
                    &MetricEvent {
                        ts: now_unix_secs(),
                        tool: "cache_bust".into(),
                        action: target.clone(),
                        raw_tokens_est: 0,
                        kept_tokens_est: 0,
                        saved_tokens_est: 0,
                        elided_lines: 0,
                        loop_detected: false,
                        spilled_log: false,
                    },
                );
            }
            if outcome.chars_saved > 0 {
                if let Ok(rewritten) = serde_json::to_vec(&json_body) {
                    let saved_tok = est_tokens_from_chars(outcome.chars_saved);
                    eprintln!(
                        "[eai proxy] Masked {} observations (boundary={}) -> saved ~{} tok",
                        outcome.masked_count, outcome.active_boundary, saved_tok
                    );
                    append_event(
                        &cwd,
                        &MetricEvent {
                            ts: now_unix_secs(),
                            tool: "proxy_mask".into(),
                            action: format!("B{}", outcome.active_boundary),
                            raw_tokens_est: saved_tok,
                            kept_tokens_est: 0,
                            saved_tokens_est: saved_tok,
                            elided_lines: outcome.masked_count,
                            loop_detected: false,
                            spilled_log: false,
                        },
                    );
                    body_bytes = rewritten;
                }
            }
        }
    }

    let url = format!("{}{}", upstream_base.trim_end_matches('/'), target);
    let resp_res = if method.eq_ignore_ascii_case("POST") {
        let mut req = agent.post(&url);
        for (k, v) in &headers {
            if !k.eq_ignore_ascii_case("host")
                && !k.eq_ignore_ascii_case("content-length")
                && !k.eq_ignore_ascii_case("transfer-encoding")
                && !k.eq_ignore_ascii_case("accept-encoding")
            {
                req = req.header(k, v);
            }
        }
        req.send(&body_bytes[..])
    } else {
        let mut req = agent.get(&url);
        for (k, v) in &headers {
            if !k.eq_ignore_ascii_case("host") && !k.eq_ignore_ascii_case("accept-encoding") {
                req = req.header(k, v);
            }
        }
        req.call()
    };

    let upstream_resp = match resp_res {
        Ok(r) => r,
        Err(e) => {
            let msg = format!("{{\"error\":\"eai proxy upstream error: {}\"}}", e);
            let hdr = format!(
                "HTTP/1.1 502 Bad Gateway\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                msg.len()
            );
            let _ = client.write_all(hdr.as_bytes());
            let _ = client.write_all(msg.as_bytes());
            return;
        }
    };

    let status = upstream_resp.status();
    let status_line = format!(
        "HTTP/1.1 {} {}\r\n",
        status.as_u16(),
        status.canonical_reason().unwrap_or("OK")
    );
    let _ = client.write_all(status_line.as_bytes());

    let mut is_sse = false;
    for (k, v) in upstream_resp.headers() {
        let ks = k.as_str();
        if ks.eq_ignore_ascii_case("transfer-encoding")
            || ks.eq_ignore_ascii_case("content-length")
            || ks.eq_ignore_ascii_case("connection")
            || ks.eq_ignore_ascii_case("content-encoding")
        {
            continue;
        }
        if let Ok(vs) = v.to_str() {
            if ks.eq_ignore_ascii_case("content-type") && vs.contains("text/event-stream") {
                is_sse = true;
            }
            let _ = write!(client, "{}: {}\r\n", ks, vs);
        }
    }

    if is_sse {
        let _ = client.write_all(b"Transfer-Encoding: chunked\r\nConnection: close\r\n\r\n");
        let _ = client.flush();
        let mut body_reader = upstream_resp.into_body().into_reader();
        let mut buf = [0u8; 4096];
        loop {
            match body_reader.read(&mut buf) {
                Ok(0) | Err(_) => {
                    let _ = client.write_all(b"0\r\n\r\n");
                    let _ = client.flush();
                    break;
                }
                Ok(n) => {
                    let _ = write!(client, "{:x}\r\n", n);
                    let _ = client.write_all(&buf[..n]);
                    let _ = client.write_all(b"\r\n");
                    let _ = client.flush();
                }
            }
        }
    } else {
        let mut resp_bytes = Vec::new();
        let _ = upstream_resp
            .into_body()
            .into_reader()
            .read_to_end(&mut resp_bytes);
        let _ = write!(
            client,
            "Content-Length: {}\r\nConnection: close\r\n\r\n",
            resp_bytes.len()
        );
        let _ = client.write_all(&resp_bytes);
        let _ = client.flush();
    }
}

pub fn cmd_proxy(args: &[String]) -> i32 {
    let mut port: u16 = 7421;
    let mut provider = "anthropic".to_string();
    let mut upstream_override: Option<String> = None;
    let mut batch_k: Option<usize> = None;
    let mut keep_m: Option<usize> = None;
    let mut min_chars: usize = 480;
    let mut simulate_path: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--port" if i + 1 < args.len() => {
                port = args[i + 1].parse().unwrap_or(7421);
                i += 1;
            }
            "--provider" if i + 1 < args.len() => {
                provider = args[i + 1].to_ascii_lowercase();
                i += 1;
            }
            "--upstream" if i + 1 < args.len() => {
                upstream_override = Some(args[i + 1].clone());
                i += 1;
            }
            "--batch-k" | "-k" if i + 1 < args.len() => {
                batch_k = args[i + 1].parse().ok();
                i += 1;
            }
            "--keep-m" | "-m" if i + 1 < args.len() => {
                keep_m = args[i + 1].parse().ok();
                i += 1;
            }
            "--min-chars" if i + 1 < args.len() => {
                min_chars = args[i + 1].parse().unwrap_or(480);
                i += 1;
            }
            "--simulate" if i + 1 < args.len() => {
                simulate_path = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "-h" | "--help" => {
                println!(
                    "eai proxy — Opt-in local Cache-Aware Batch Observation Masking proxy (Exp 007)\n\n\
                     USAGE:\n\
                     \x20 eai proxy [--port 7421] [--provider anthropic|openai|gemini] [--batch-k 12] [--keep-m 6]\n\
                     \x20 eai proxy --simulate <transcript.jsonl> [--batch-k 12] [--keep-m 6]\n\n\
                     POINT YOUR AGENT AT THE LOCAL PROXY:\n\
                     \x20 Claude Code: ANTHROPIC_BASE_URL=http://127.0.0.1:7421\n\
                     \x20 Codex CLI:   OPENAI_BASE_URL=http://127.0.0.1:7421/v1"
                );
                return 0;
            }
            _ => {}
        }
        i += 1;
    }

    let mut cfg = BatchMaskConfig::for_provider(&provider);
    if let Some(k) = batch_k {
        cfg.batch_k = k.max(1);
    }
    if let Some(m) = keep_m {
        cfg.keep_m = m;
    }
    cfg.min_chars = min_chars;

    if let Some(sim_p) = simulate_path {
        return run_simulate(&sim_p, cfg);
    }

    let upstream = upstream_override.unwrap_or_else(|| match provider.as_str() {
        "openai" | "codex" => "https://api.openai.com".to_string(),
        "gemini" => "https://generativelanguage.googleapis.com".to_string(),
        _ => "https://api.anthropic.com".to_string(),
    });

    let addr = format!("127.0.0.1:{}", port);
    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Error binding proxy to {}: {}", addr, e);
            return 1;
        }
    };

    let env_hint = match provider.as_str() {
        "openai" | "codex" => format!("OPENAI_BASE_URL=http://{}/v1", addr),
        "gemini" => format!("GEMINI_API_BASE=http://{}", addr),
        _ => format!("ANTHROPIC_BASE_URL=http://{}", addr),
    };
    println!(
        "eai proxy listening on http://{} -> {} (Exp 007 batch masking K={}, M={}, min={} chars)",
        addr, upstream, cfg.batch_k, cfg.keep_m, cfg.min_chars
    );
    println!("Point your agent via: {}", env_hint);

    let tracker = Arc::new(Mutex::new(PrefixTracker::default()));
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let agent_cfg = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build();
    let http_agent = Arc::new(ureq::Agent::new_with_config(agent_cfg));

    for stream in listener.incoming().flatten() {
        let up = upstream.clone();
        let tr = Arc::clone(&tracker);
        let dir = cwd.clone();
        let ag = Arc::clone(&http_agent);
        thread::spawn(move || {
            handle_proxy_connection(stream, up, cfg, tr, dir, ag);
        });
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_anthropic_body(num_obs: usize, sys_text: &str) -> Value {
        let big_payload = "x".repeat(800);
        let mut msgs = Vec::new();
        for i in 0..num_obs {
            msgs.push(json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": format!("t{}", i),
                    "content": big_payload
                }]
            }));
            msgs.push(json!({
                "role": "assistant",
                "content": [{"type": "text", "text": "ok"}]
            }));
        }
        json!({
            "model": "claude-sonnet-5",
            "system": sys_text,
            "messages": msgs
        })
    }

    #[test]
    fn test_exp007_prefix_stability_and_cache_bust_detection() {
        let cfg = BatchMaskConfig {
            batch_k: 4,
            keep_m: 2,
            min_chars: 400,
        };
        let mut tracker = PrefixTracker::default();

        // 1. Before K+M (=6), zero observations masked
        let mut b5 = make_anthropic_body(5, "static system");
        let o5 = apply_batch_masking(&mut b5, cfg, Some(&mut tracker));
        assert_eq!(o5.masked_count, 0);
        assert!(!o5.cache_bust_detected);

        // 2. At K+M (=6), first 4 observations masked with batch B1
        let mut b6 = make_anthropic_body(6, "static system");
        let o6 = apply_batch_masking(&mut b6, cfg, Some(&mut tracker));
        assert_eq!(o6.masked_count, 4);
        assert_eq!(o6.active_boundary, 4);
        assert!(o6.chars_saved > 2500);
        let snap_first_4: Vec<Value> = b6["messages"].as_array().unwrap()[..8].to_vec();

        // 3. At 10 observations (boundary advances to 8), first 4 observations remain 100% BYTE-IDENTICAL!
        let mut b10 = make_anthropic_body(10, "static system");
        let o10 = apply_batch_masking(&mut b10, cfg, Some(&mut tracker));
        assert_eq!(o10.masked_count, 8);
        assert_eq!(b10["messages"].as_array().unwrap()[..8].to_vec(), snap_first_4);
        assert!(!o10.cache_bust_detected);

        // 4. Mutating system prompt mid-session triggers cache-bust alert!
        let mut b10_bust = make_anthropic_body(10, "dynamic system 12:00:01");
        let o_bust = apply_batch_masking(&mut b10_bust, cfg, Some(&mut tracker));
        assert!(o_bust.cache_bust_detected);
    }

    #[test]
    fn test_batch_masking_beats_naive_on_claude_cost() {
        let obs = vec![1800u64; 40];
        let rows = simulate_masking_trajectory(6000, &obs, "claude", 12, 6, 120);
        let base_cost = rows[0].cost_weighted;
        let naive_cost = rows[1].cost_weighted;
        let batch_cost = rows[2].cost_weighted;
        // Naive per-turn masking MUST be worse than baseline on Claude, while batch masking MUST beat baseline!
        assert!(naive_cost > base_cost, "naive {} should exceed base {}", naive_cost, base_cost);
        assert!(batch_cost < base_cost, "batch {} should beat base {}", batch_cost, base_cost);
    }

    #[test]
    fn test_live_http_loopback_proxy_masks_request() {
        // 1. Start mock upstream server that echoes the received JSON body back
        let upstream_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let up_addr = upstream_listener.local_addr().unwrap();
        let up_handle = thread::spawn(move || {
            let (mut s, _) = upstream_listener.accept().unwrap();
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            let mut cl = 0usize;
            loop {
                let mut h = String::new();
                r.read_line(&mut h).unwrap();
                if h == "\r\n" || h == "\n" || h.is_empty() {
                    break;
                }
                if let Some((k, v)) = h.split_once(':') {
                    if k.eq_ignore_ascii_case("content-length") {
                        cl = v.trim().parse().unwrap_or(0);
                    }
                }
            }
            let mut body = vec![0u8; cl];
            r.read_exact(&mut body).unwrap();
            let resp_hdr = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            s.write_all(resp_hdr.as_bytes()).unwrap();
            s.write_all(&body).unwrap();
        });

        // 2. Start 1-request proxy server pointing at mock upstream
        let proxy_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let px_addr = proxy_listener.local_addr().unwrap();
        let px_handle = thread::spawn(move || {
            let (client, _) = proxy_listener.accept().unwrap();
            let cfg = BatchMaskConfig {
                batch_k: 4,
                keep_m: 2,
                min_chars: 400,
            };
            let tracker = Arc::new(Mutex::new(PrefixTracker::default()));
            let agent = Arc::new(ureq::Agent::new_with_config(
                ureq::Agent::config_builder()
                    .http_status_as_error(false)
                    .build(),
            ));
            handle_proxy_connection(
                client,
                format!("http://{}", up_addr),
                cfg,
                tracker,
                std::env::temp_dir(),
                agent,
            );
        });

        // 3. Send 6-observation Anthropic request through proxy
        let req_body = make_anthropic_body(6, "static system");
        let resp = ureq::post(&format!("http://{}/v1/messages", px_addr))
            .header("Content-Type", "application/json")
            .send(serde_json::to_string(&req_body).unwrap())
            .unwrap();
        assert_eq!(resp.status().as_u16(), 200);
        let mut echoed = String::new();
        resp.into_body().into_reader().read_to_string(&mut echoed).unwrap();
        let echoed_json: Value = serde_json::from_str(&echoed).unwrap();

        // First 4 observations must have been masked; last 2 must remain verbatim (800 chars)
        let first_obs = echoed_json["messages"][0]["content"][0]["content"]
            .as_str()
            .unwrap();
        let last_obs = echoed_json["messages"][10]["content"][0]["content"]
            .as_str()
            .unwrap();
        assert!(first_obs.starts_with("[eai masked:"));
        assert_eq!(last_obs.len(), 800);

        px_handle.join().unwrap();
        up_handle.join().unwrap();
    }
}
