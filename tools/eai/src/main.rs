//! `eai` — Unified zero-dependency CLI for efficient-ai.

mod dashboard;
mod hooks;
mod impact;
mod mcp;
mod proxy;
mod setup;
mod telemetry;
mod tokscope;

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn print_help() {
    println!(
        "eai {} — Same quality, fewer tokens (Claude Code, Antigravity, Gemini CLI, Codex, Cursor)\n\
         \n\
         SETUP & HEALTH:\n\
         \x20 eai init [--project <dir>] [--global] [--mcp] [--agents auto|...]\n\
         \x20 eai doctor [--project <dir>]                     Verify hooks, shims, latency & prefix health\n\
         \x20 eai pause / eai resume [--project <dir>]         Temporarily pause or resume hook rewriting\n\
         \x20 eai uninstall [--project <dir>] [--global]       Cleanly remove efficient-ai hooks & shims\n\
         \n\
         MONITORING & IMPACT PROOF:\n\
         \x20 eai impact [--project <dir>] [--since 30d]       Before vs. After project impact & advisor report\n\
         \x20 eai dashboard [--port 7420] [--since 30d]        Launch local offline web UI (127.0.0.1:7420)\n\
         \x20 eai watch [--project <dir>] [--once]             Live terminal watcher with cache-bust alerts\n\
         \x20 eai share [--out eai-efficiency-card.svg]        Generate privacy-safe SVG efficiency proof card\n\
         \x20 eai stats [--project <dir>] [--global] [--json]  Local savings, recovery rate & baseline metrics\n\
         \x20 eai analyze [path] [--agent auto] [--since 14d]  Multi-agent token, cache & USD cost profiler\n\
         \x20 eai audit [path] [--include-user] [--fix]        Audit & auto-minify static prefix configs\n\
         \n\
         CORE TOOLS & INTEGRATIONS:\n\
         \x20 eai outline <file|dir>... [--symbol S] [--budget N]  Tree-sitter outlines & symbol reads\n\
         \x20 eai squeeze [--max-lines N] -- <cmd> [args...]       Lossless compression, spill log & loop guard\n\
         \x20 eai proxy [--port 7421] [--simulate <file>]          Exp 007 cache-aware batch masking proxy\n\
         \x20 eai hook <claude|antigravity|gemini>                 Sub-10ms native agent hook router (stdin JSON)\n\
         \x20 eai mcp                                              Ultra-lean stdio MCP server (<280 idle tokens)\n\
         \x20 eai sanitize [path] --out <file.json>                Export privacy-safe aggregate session JSON\n\
         \x20 eai selftest                                         Verify all 4 agent parsers on synthetic fixtures",
        env!("CARGO_PKG_VERSION")
    );
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || matches!(args[0].as_str(), "-h" | "--help" | "help") {
        print_help();
        return ExitCode::SUCCESS;
    }
    if matches!(args[0].as_str(), "-V" | "--version" | "version") {
        println!("eai {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }

    let sub = args[0].as_str();
    let rest = &args[1..];
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    match sub {
        "outline" => {
            let (code, metrics) = outline::run_cli(rest);
            if let Some(m) = metrics {
                telemetry::record_outline(&cwd, m.action, m.raw_chars, m.output_chars);
            }
            code
        }
        "squeeze" => {
            let (code, metrics) = squeeze::run_cli(rest);
            if let Some(m) = metrics {
                telemetry::record_squeeze(
                    &cwd,
                    &m.command,
                    m.raw_chars,
                    m.kept_chars,
                    m.elided_lines,
                    m.log_path.is_some(),
                    m.loop_detected,
                );
            }
            code
        }
        "hook" => ExitCode::from(hooks::cmd_hook(rest) as u8),
        "mcp" => ExitCode::from(mcp::cmd_mcp(rest) as u8),
        "proxy" => ExitCode::from(proxy::cmd_proxy(rest) as u8),
        "pause" => ExitCode::from(setup::cmd_pause(rest, true) as u8),
        "resume" => ExitCode::from(setup::cmd_pause(rest, false) as u8),
        "analyze" => ExitCode::from(tokscope::cmd_analyze(rest) as u8),
        "audit" => ExitCode::from(tokscope::cmd_audit(rest) as u8),
        "sanitize" => ExitCode::from(tokscope::cmd_sanitize(rest) as u8),
        "selftest" => ExitCode::from(tokscope::cmd_selftest() as u8),
        "init" | "setup" => ExitCode::from(setup::cmd_init(rest) as u8),
        "doctor" => ExitCode::from(setup::cmd_doctor(rest) as u8),
        "stats" => ExitCode::from(telemetry::cmd_stats(rest) as u8),
        "impact" => ExitCode::from(impact::cmd_impact(rest) as u8),
        "share" => ExitCode::from(impact::cmd_share(rest) as u8),
        "watch" => ExitCode::from(impact::cmd_watch(rest) as u8),
        "dashboard" => ExitCode::from(dashboard::cmd_dashboard(rest) as u8),
        "uninstall" => ExitCode::from(setup::cmd_uninstall(rest) as u8),
        other => {
            eprintln!("Unknown command '{}'. Run `eai --help` for usage.", other);
            ExitCode::from(2)
        }
    }
}
