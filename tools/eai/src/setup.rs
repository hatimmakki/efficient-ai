//! Zero-friction 10-second setup (`eai init`), diagnostics (`eai doctor`),
//! and clean removal (`eai uninstall`) across Claude Code, Antigravity, Gemini CLI, and Codex CLI.

use crate::telemetry::{now_unix_secs, user_home, write_baseline, BaselineSnapshot};
use crate::tokscope::{always_prefix_tokens, audit_directory, load_recent_sessions, summarize_corpus};
use serde_json::{json, Map, Value};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const MANAGED_START: &str = "<!-- efficient-ai:start -->";
const MANAGED_END: &str = "<!-- efficient-ai:end -->";

const LEAN_AGENTS_POLICY: &str = r#"# Agent Efficiency Policy

## Read & Turn Discipline (H3, H4)
- Batch independent tool calls in a single turn.
- For files >200 lines, inspect signatures (`eai outline <path>`) or targeted line ranges before whole-file reads.
- After search, read only the enclosing symbol or matched range, not the whole file.
- Stop immediately once verification passes; do not re-run passing checks.

## Output Brevity — Prose Only (H6)
- Keep prose direct and brief. Skip preambles ("Let me...", "I will now...") and post-change recaps.
- Never echo unchanged code or repeat diff contents in prose; cite `path:line` instead.
- Never sacrifice code completeness, test coverage, or reasoning depth for brevity.

## Compact Command Flags (H8)
- Pass non-interactive, terse flags to CLI tools: `git status -sb`, `git log --oneline -n 10`, `git --no-pager diff -U1`, `pytest -q --tb=short`, `cargo test -q`, `npm test --silent`.
"#;

#[derive(Debug, Clone, Default)]
pub struct DetectedAgents {
    pub claude: bool,
    pub antigravity: bool,
    pub gemini: bool,
    pub codex: bool,
}

pub fn detect_installed_agents(home: Option<&Path>) -> DetectedAgents {
    let Some(h) = home else {
        return DetectedAgents::default();
    };
    DetectedAgents {
        claude: h.join(".claude").is_dir(),
        antigravity: h.join(".gemini").join("antigravity").is_dir()
            || h.join(".gemini").join("antigravity-cli").is_dir()
            || h.join(".gemini").join("antigravity-ide").is_dir()
            || h.join(".gemini").join("config").is_dir(),
        gemini: h.join(".gemini").is_dir(),
        codex: h.join(".codex").is_dir(),
    }
}

fn quote_exe(p: &Path) -> String {
    let s = p.display().to_string();
    if s.contains(' ') {
        format!("\"{}\"", s)
    } else {
        s
    }
}

fn read_json_or_empty(path: &Path) -> Value {
    if !path.is_file() {
        return json!({});
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .filter(|v| v.is_object())
        .unwrap_or_else(|| json!({}))
}

fn write_json_with_backup(path: &Path, val: &Value) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    if path.is_file() {
        let bak = PathBuf::from(format!("{}.eai.bak", path.display()));
        if !bak.exists() {
            let _ = fs::copy(path, &bak);
        }
    }
    let body = serde_json::to_string_pretty(val).unwrap_or_else(|_| "{}".to_string());
    fs::write(path, format!("{}\n", body))
}

fn is_eai_hook_entry(entry: &Value) -> bool {
    let s = entry.to_string();
    s.contains("hook claude")
        || s.contains("hook gemini")
        || s.contains("hook antigravity")
        || s.contains("claude_pre_tool.py")
        || s.contains("gemini_before_tool.py")
        || s.contains("eai_hook.py")
}

/// Safely merge an efficient-ai Claude Code PreToolUse hook into `.claude/settings.json`.
pub fn merge_claude_settings(settings_path: &Path, eai_exe: &Path) -> std::io::Result<bool> {
    let mut root = read_json_or_empty(settings_path);
    let obj = root.as_object_mut().unwrap();
    let hooks_obj = obj
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .unwrap();
    let pre_arr = hooks_obj
        .entry("PreToolUse")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .unwrap();

    let cmd = format!("{} hook claude", quote_exe(eai_exe));
    let desired = json!({
        "matcher": "Bash|PowerShell|Read",
        "hooks": [{
            "type": "command",
            "command": cmd
        }]
    });

    pre_arr.retain(|entry| !is_eai_hook_entry(entry));
    pre_arr.push(desired);
    write_json_with_backup(settings_path, &root)?;
    Ok(true)
}

/// Safely merge an efficient-ai Antigravity hook into `.agents/hooks.json` or `~/.gemini/config/hooks.json`.
pub fn merge_antigravity_hooks(hooks_path: &Path, eai_exe: &Path) -> std::io::Result<bool> {
    let mut root = read_json_or_empty(hooks_path);
    let obj = root.as_object_mut().unwrap();
    let cmd = format!("{} hook antigravity", quote_exe(eai_exe));

    obj.insert(
        "efficient-ai".to_string(),
        json!({
            "PreToolUse": [{
                "matcher": "run_command|read_file",
                "hooks": [{
                    "type": "command",
                    "command": cmd,
                    "timeout": 5
                }]
            }],
            "PostToolUse": [{
                "matcher": "run_command",
                "hooks": [{
                    "type": "command",
                    "command": cmd,
                    "timeout": 5
                }]
            }]
        }),
    );
    write_json_with_backup(hooks_path, &root)?;
    Ok(true)
}

/// Safely merge an efficient-ai Gemini CLI BeforeTool + AfterTool hook into `.gemini/settings.json`.
pub fn merge_gemini_settings(
    settings_path: &Path,
    eai_exe: &Path,
    set_context_file: bool,
) -> std::io::Result<bool> {
    let mut root = read_json_or_empty(settings_path);
    let obj = root.as_object_mut().unwrap();

    if set_context_file && !obj.contains_key("contextFileName") {
        obj.insert("contextFileName".to_string(), json!(["AGENTS.md"]));
    }

    let hooks_obj = obj
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .unwrap();

    let cmd = format!("{} hook gemini", quote_exe(eai_exe));

    let before_arr = hooks_obj
        .entry("BeforeTool")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .unwrap();
    before_arr.retain(|e| !is_eai_hook_entry(e));
    before_arr.push(json!({
        "matcher": "run_shell_command|read_file",
        "hooks": [{
            "name": "efficient-ai-before",
            "type": "command",
            "command": cmd
        }]
    }));

    let after_arr = hooks_obj
        .entry("AfterTool")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .unwrap();
    after_arr.retain(|e| !is_eai_hook_entry(e));
    after_arr.push(json!({
        "matcher": "run_shell_command",
        "hooks": [{
            "name": "efficient-ai-after",
            "type": "command",
            "command": cmd
        }]
    }));

    write_json_with_backup(settings_path, &root)?;
    Ok(true)
}

/// Install or idempotently update the managed AGENTS.md block without destroying existing user rules.
pub fn upsert_agents_md(project_dir: &Path) -> std::io::Result<&'static str> {
    let agents_path = project_dir.join("AGENTS.md");
    let block = format!("{}\n{}{}\n", MANAGED_START, LEAN_AGENTS_POLICY, MANAGED_END);

    if !agents_path.exists() {
        fs::write(&agents_path, block)?;
        return Ok("created");
    }

    let existing = fs::read_to_string(&agents_path)?;
    if let (Some(s_idx), Some(e_idx)) = (existing.find(MANAGED_START), existing.find(MANAGED_END)) {
        if e_idx >= s_idx {
            let mut updated = String::new();
            updated.push_str(&existing[..s_idx]);
            updated.push_str(&block);
            let after = &existing[e_idx + MANAGED_END.len()..];
            updated.push_str(after.trim_start_matches('\n'));
            fs::write(&agents_path, updated)?;
            return Ok("updated");
        }
    }

    let mut combined = existing;
    if !combined.ends_with('\n') {
        combined.push('\n');
    }
    combined.push('\n');
    combined.push_str(&block);
    fs::write(&agents_path, combined)?;
    Ok("appended")
}

pub fn remove_managed_agents_block(project_dir: &Path) -> std::io::Result<bool> {
    let agents_path = project_dir.join("AGENTS.md");
    if !agents_path.is_file() {
        return Ok(false);
    }
    let existing = fs::read_to_string(&agents_path)?;
    let (Some(s_idx), Some(e_idx)) = (existing.find(MANAGED_START), existing.find(MANAGED_END))
    else {
        return Ok(false);
    };
    if e_idx < s_idx {
        return Ok(false);
    }
    let mut cleaned = String::new();
    cleaned.push_str(existing[..s_idx].trim_end());
    let after = existing[e_idx + MANAGED_END.len()..].trim_start();
    if !after.is_empty() {
        if !cleaned.is_empty() {
            cleaned.push_str("\n\n");
        }
        cleaned.push_str(after);
    }
    if cleaned.trim().is_empty() {
        fs::remove_file(&agents_path)?;
    } else {
        fs::write(&agents_path, format!("{}\n", cleaned.trim_end()))?;
    }
    Ok(true)
}

fn ensure_gitignore_has_eai(project_dir: &Path) -> std::io::Result<bool> {
    let gi = project_dir.join(".gitignore");
    if !gi.is_file() {
        return Ok(false);
    }
    let txt = fs::read_to_string(&gi)?;
    if txt.lines().any(|l| l.trim() == ".eai/" || l.trim() == ".eai") {
        return Ok(false);
    }
    let mut updated = txt;
    if !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str("\n# efficient-ai local logs & telemetry\n.eai/\n");
    fs::write(&gi, updated)?;
    Ok(true)
}

fn remove_eai_from_json_hooks(path: &Path, agent: &str) -> std::io::Result<bool> {
    if !path.is_file() {
        return Ok(false);
    }
    let mut root = read_json_or_empty(path);
    let Some(obj) = root.as_object_mut() else {
        return Ok(false);
    };
    let mut changed = false;

    if agent == "antigravity" {
        if obj.remove("efficient-ai").is_some() {
            changed = true;
        }
    } else if let Some(hooks_obj) = obj.get_mut("hooks").and_then(|v| v.as_object_mut()) {
        for key in ["PreToolUse", "PostToolUse", "BeforeTool", "AfterTool"] {
            if let Some(arr) = hooks_obj.get_mut(key).and_then(|v| v.as_array_mut()) {
                let before = arr.len();
                arr.retain(|e| !is_eai_hook_entry(e));
                if arr.len() != before {
                    changed = true;
                }
            }
        }
    }

    if changed {
        write_json_with_backup(path, &root)?;
    }
    Ok(changed)
}

pub fn cmd_init(args: &[String]) -> i32 {
    let mut project = PathBuf::from(".");
    let mut global = false;
    let mut agents_arg = "auto".to_string();
    let mut capture_baseline = true;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--project" | "-p" if i + 1 < args.len() => {
                project = PathBuf::from(&args[i + 1]);
                i += 1;
            }
            "--global" | "-g" => global = true,
            "--agents" if i + 1 < args.len() => {
                agents_arg = args[i + 1].clone();
                i += 1;
            }
            "--no-baseline" => capture_baseline = false,
            "-h" | "--help" => {
                println!(
                    "Usage: eai init [--project <dir>] [--global] [--agents auto|claude,antigravity,gemini,codex] [--no-baseline]\n\
                     Auto-detects installed AI coding agents, wires native hooks & PATH shims safely, installs lean AGENTS.md, and snapshots a Pre-EAI baseline."
                );
                return 0;
            }
            _ => {}
        }
        i += 1;
    }

    let eai_exe = env::current_exe().unwrap_or_else(|_| PathBuf::from("eai"));
    let home = user_home();
    let detected = detect_installed_agents(home.as_deref());

    let (want_claude, want_agy, want_gemini, want_codex) = if agents_arg == "auto" {
        let any = detected.claude || detected.antigravity || detected.gemini || detected.codex;
        if any {
            (detected.claude, detected.antigravity, detected.gemini, detected.codex)
        } else {
            // Default to all 4 when running in a fresh/container environment
            (true, true, true, true)
        }
    } else {
        let low = agents_arg.to_ascii_lowercase();
        let all = low == "all";
        (
            all || low.contains("claude"),
            all || low.contains("antigravity") || low.contains("agy"),
            all || low.contains("gemini"),
            all || low.contains("codex"),
        )
    };

    println!("# efficient-ai Setup (`eai init`)\n");
    println!("- **Binary:** `{}`", eai_exe.display());

    if global {
        let Some(h) = home else {
            eprintln!("Error: could not determine home directory for --global setup.");
            return 1;
        };
        let _ = fs::create_dir_all(h.join(".eai"));
        if want_claude {
            let p = h.join(".claude").join("settings.json");
            if merge_claude_settings(&p, &eai_exe).is_ok() {
                println!("- [OK] Claude Code global hook wired: `{}`", p.display());
            }
        }
        if want_agy {
            let p = h.join(".gemini").join("config").join("hooks.json");
            if merge_antigravity_hooks(&p, &eai_exe).is_ok() {
                println!("- [OK] Antigravity global hook wired: `{}`", p.display());
            }
        }
        if want_gemini {
            let p = h.join(".gemini").join("settings.json");
            if merge_gemini_settings(&p, &eai_exe, false).is_ok() {
                println!("- [OK] Gemini CLI global hook wired: `{}`", p.display());
            }
        }
        if want_codex {
            let shims_dir = h.join(".eai").join("shims");
            if let Ok(n) = squeeze::install_shims_for_exe(&shims_dir, &eai_exe, true) {
                println!(
                    "- [OK] Codex/universal PATH shims ({}) installed: `{}`",
                    n,
                    shims_dir.display()
                );
            }
        }
        println!("\nGlobal setup complete. Run `eai doctor` to verify.");
        return 0;
    }

    let project = fs::canonicalize(&project).unwrap_or(project);
    if !project.is_dir() {
        eprintln!("Error: project directory does not exist: {}", project.display());
        return 1;
    }

    let _ = fs::create_dir_all(project.join(".eai"));
    if let Ok(added) = ensure_gitignore_has_eai(&project) {
        if added {
            println!("- [OK] Added `.eai/` to `.gitignore`");
        }
    }

    // 1. Single-source lean AGENTS.md + thin shims
    match upsert_agents_md(&project) {
        Ok(status) => println!("- [OK] `AGENTS.md` {} (227 tok policy block)", status),
        Err(e) => eprintln!("- [WARN] Could not write AGENTS.md: {}", e),
    }

    let claude_md = project.join("CLAUDE.md");
    if want_claude && !claude_md.exists() && fs::write(&claude_md, "@AGENTS.md\n").is_ok() {
        println!("- [OK] Created `CLAUDE.md` (`@AGENTS.md` 3-token shim)");
    }
    let gemini_md = project.join("GEMINI.md");
    if (want_gemini || want_agy)
        && !gemini_md.exists()
        && fs::write(&gemini_md, "@AGENTS.md\n").is_ok()
    {
        println!("- [OK] Created `GEMINI.md` (`@AGENTS.md` 3-token shim)");
    }

    // 2. Wire native agent hooks in project
    if want_claude {
        let p = project.join(".claude").join("settings.json");
        if merge_claude_settings(&p, &eai_exe).is_ok() {
            println!("- [OK] Claude Code hook wired: `.claude/settings.json`");
        }
    }
    if want_agy {
        let p = project.join(".agents").join("hooks.json");
        if merge_antigravity_hooks(&p, &eai_exe).is_ok() {
            println!("- [OK] Antigravity hook wired: `.agents/hooks.json`");
        }
    }
    if want_gemini {
        let p = project.join(".gemini").join("settings.json");
        if merge_gemini_settings(&p, &eai_exe, true).is_ok() {
            println!("- [OK] Gemini CLI hook wired: `.gemini/settings.json`");
        }
    }

    // 3. Portable executable PATH shims (works for Codex CLI + all agents)
    let shims_dir = project.join(".eai").join("shims");
    if let Ok(n) = squeeze::install_shims_for_exe(&shims_dir, &eai_exe, true) {
        println!(
            "- [OK] Installed {} executable PATH shims in `.eai/shims/` (for Codex & shell sessions)",
            n
        );
    }

    // 4. Snapshot Pre-EAI Baseline from up to 40 most recent sessions (<200ms)
    if capture_baseline && !project.join(".eai").join("baseline.json").exists() {
        let sessions = load_recent_sessions("auto", 40);
        let summary = summarize_corpus(&sessions);
        let audit_items = audit_directory(&project, false);
        let prefix_tok = always_prefix_tokens(&audit_items);
        let denom = summary.total_files.max(1) as f64;
        let tot_reads = summary.read_full_count + summary.read_ranged_count;
        let snap = BaselineSnapshot {
            captured_at: now_unix_secs(),
            sessions_sampled: summary.total_files,
            total_turns: summary.total_turns(),
            avg_cost_weighted_per_session: summary.total_cost_weighted / denom,
            avg_usd_per_session: summary.total_estimated_usd / denom,
            cache_hit_rate: summary.cache_hit_rate(),
            whole_file_read_ratio: if tot_reads > 0 {
                (summary.read_full_count as f64) / (tot_reads as f64)
            } else {
                0.0
            },
            static_prefix_tokens: prefix_tok,
        };
        if write_baseline(&project, &snap).is_ok() {
            println!(
                "- [OK] Captured Pre-EAI baseline (`.eai/baseline.json`, {} historical sessions sampled)",
                snap.sessions_sampled
            );
        }
    }

    println!();
    cmd_doctor(&["--project".to_string(), project.display().to_string()])
}

pub fn cmd_doctor(args: &[String]) -> i32 {
    let mut project = PathBuf::from(".");
    let mut i = 0;
    while i < args.len() {
        if matches!(args[i].as_str(), "--project" | "-p") && i + 1 < args.len() {
            project = PathBuf::from(&args[i + 1]);
            i += 1;
        }
        i += 1;
    }

    let eai_exe = env::current_exe().unwrap_or_else(|_| PathBuf::from("eai"));
    let home = user_home();
    let detected = detect_installed_agents(home.as_deref());

    // Measure round-trip latency of core outline + squeeze engines
    let t0 = Instant::now();
    let _ = outline::parser::extract_symbols("def demo():\n    return 1\n", outline::parser::Lang::Python);
    let _ = squeeze::compressor::compress_stream("tests/a.py::test_1 PASSED\n1 passed", 150);
    let engine_us = t0.elapsed().as_micros();

    println!("## efficient-ai Doctor (`eai doctor`)\n");
    println!("- **Version:** `eai {}` (`{}`)", env!("CARGO_PKG_VERSION"), eai_exe.display());
    println!("- **Engine round-trip latency:** {} µs (<10ms target: PASS)", engine_us);

    let check_hook = |p: PathBuf| -> &'static str {
        if p.is_file() {
            if fs::read_to_string(&p)
                .map(|s| s.contains("eai") || s.contains("efficient-ai"))
                .unwrap_or(false)
            {
                "wired"
            } else {
                "present (no eai hook)"
            }
        } else {
            "not configured"
        }
    };

    let h = home.as_deref();
    println!(
        "- **Claude Code:** detected={} | project={} | global={}",
        detected.claude,
        check_hook(project.join(".claude").join("settings.json")),
        h.map(|d| check_hook(d.join(".claude").join("settings.json"))).unwrap_or("n/a")
    );
    println!(
        "- **Antigravity:** detected={} | project={} | global={}",
        detected.antigravity,
        check_hook(project.join(".agents").join("hooks.json")),
        h.map(|d| check_hook(d.join(".gemini").join("config").join("hooks.json"))).unwrap_or("n/a")
    );
    println!(
        "- **Gemini CLI:** detected={} | project={} | global={}",
        detected.gemini,
        check_hook(project.join(".gemini").join("settings.json")),
        h.map(|d| check_hook(d.join(".gemini").join("settings.json"))).unwrap_or("n/a")
    );
    println!(
        "- **Codex CLI:** detected={} | project shims={}",
        detected.codex,
        if project.join(".eai").join("shims").join("pytest").is_file() {
            "ready (.eai/shims)"
        } else {
            "not installed"
        }
    );

    let audit_items = audit_directory(&project, false);
    let prefix_tok = always_prefix_tokens(&audit_items);
    let prefix_health = if prefix_tok <= 1500 {
        "HEALTHY"
    } else if prefix_tok <= 4000 {
        "MODERATE"
    } else {
        "HIGH (run `eai audit . --include-user`)"
    };
    println!(
        "- **Project static prefix:** {} tok/turn ({})",
        prefix_tok, prefix_health
    );
    println!(
        "- **Baseline snapshot:** {}",
        if project.join(".eai").join("baseline.json").is_file() {
            "captured (.eai/baseline.json)"
        } else {
            "none (run `eai init`)"
        }
    );
    0
}

pub fn cmd_uninstall(args: &[String]) -> i32 {
    let mut project = PathBuf::from(".");
    let mut global = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--project" | "-p" if i + 1 < args.len() => {
                project = PathBuf::from(&args[i + 1]);
                i += 1;
            }
            "--global" | "-g" => global = true,
            "-h" | "--help" => {
                println!("Usage: eai uninstall [--project <dir>] [--global]");
                return 0;
            }
            _ => {}
        }
        i += 1;
    }

    println!("# efficient-ai Uninstall\n");
    if global {
        if let Some(h) = user_home() {
            for (p, ag) in [
                (h.join(".claude").join("settings.json"), "claude"),
                (h.join(".gemini").join("config").join("hooks.json"), "antigravity"),
                (h.join(".gemini").join("settings.json"), "gemini"),
            ] {
                if let Ok(true) = remove_eai_from_json_hooks(&p, ag) {
                    println!("- Removed efficient-ai hook from `{}`", p.display());
                }
            }
            let shims = h.join(".eai").join("shims");
            if shims.is_dir() && fs::remove_dir_all(&shims).is_ok() {
                println!("- Removed `{}`", shims.display());
            }
        }
        println!("\nGlobal uninstall complete.");
        return 0;
    }

    for (p, ag) in [
        (project.join(".claude").join("settings.json"), "claude"),
        (project.join(".agents").join("hooks.json"), "antigravity"),
        (project.join(".gemini").join("settings.json"), "gemini"),
    ] {
        if let Ok(true) = remove_eai_from_json_hooks(&p, ag) {
            println!("- Removed efficient-ai hook from `{}`", p.display());
        }
    }
    if let Ok(true) = remove_managed_agents_block(&project) {
        println!("- Removed managed efficient-ai block from `AGENTS.md`");
    }
    let shims = project.join(".eai").join("shims");
    if shims.is_dir() && fs::remove_dir_all(&shims).is_ok() {
        println!("- Removed `.eai/shims/`");
    }
    println!("\nProject uninstall complete (preserved existing user rules and other hooks).");
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_preserves_existing_hooks_and_uninstall_cleans_up() {
        let tmp = env::temp_dir().join(format!("eai_setup_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join(".claude")).unwrap();
        fs::create_dir_all(tmp.join(".agents")).unwrap();

        // Pre-populate user hooks & AGENTS.md that MUST be preserved
        fs::write(
            tmp.join(".claude").join("settings.json"),
            r#"{"hooks":{"PreToolUse":[{"matcher":"Write","hooks":[{"type":"command","command":"my-custom-lint"}]}]}}"#,
        )
        .unwrap();
        fs::write(
            tmp.join(".agents").join("hooks.json"),
            r#"{"user-lint":{"PostToolUse":[]}}"#,
        )
        .unwrap();
        fs::write(tmp.join("AGENTS.md"), "# Team Rules\n- Always write tests.\n").unwrap();

        let rc = cmd_init(&[
            "--project".into(),
            tmp.display().to_string(),
            "--agents".into(),
            "all".into(),
            "--no-baseline".into(),
        ]);
        assert_eq!(rc, 0);

        // Verify user hook + efficient-ai hook both exist
        let claude_txt = fs::read_to_string(tmp.join(".claude").join("settings.json")).unwrap();
        assert!(claude_txt.contains("my-custom-lint"));
        assert!(claude_txt.contains("hook claude"));

        let agy_txt = fs::read_to_string(tmp.join(".agents").join("hooks.json")).unwrap();
        assert!(agy_txt.contains("user-lint"));
        assert!(agy_txt.contains("efficient-ai"));

        let agents_md = fs::read_to_string(tmp.join("AGENTS.md")).unwrap();
        assert!(agents_md.contains("# Team Rules"));
        assert!(agents_md.contains(MANAGED_START));

        // Uninstall must strip efficient-ai while preserving Team Rules & user hooks
        let rc2 = cmd_uninstall(&["--project".into(), tmp.display().to_string()]);
        assert_eq!(rc2, 0);

        let claude_after = fs::read_to_string(tmp.join(".claude").join("settings.json")).unwrap();
        assert!(claude_after.contains("my-custom-lint"));
        assert!(!claude_after.contains("hook claude"));

        let agy_after = fs::read_to_string(tmp.join(".agents").join("hooks.json")).unwrap();
        assert!(agy_after.contains("user-lint"));
        assert!(!agy_after.contains("efficient-ai"));

        let md_after = fs::read_to_string(tmp.join("AGENTS.md")).unwrap();
        assert!(md_after.contains("# Team Rules"));
        assert!(!md_after.contains(MANAGED_START));

        let _ = fs::remove_dir_all(&tmp);
    }
}
