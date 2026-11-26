//! `eai` — Unified zero-dependency CLI for efficient-ai.

mod dashboard;
mod hooks;
mod impact;
mod mcp;
mod pricing;
mod proxy;
mod setup;
mod statusline;
mod telemetry;
mod tokscope;
mod update;

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn print_help() {
    println!(
        "eai {} — Same quality, fewer tokens (Claude Code, Antigravity, Gemini CLI, Codex, Cursor)\n\
         \n\
         SETUP & HEALTH:\n\
         \x20 eai init [--project <dir>] [--global] [--mcp] [--no-statusline] [--agents auto|...]\n\
         \x20 eai statusline [install|off|preview]             Zero-token live bottom bar (Antigravity & Claude Code)\n\
         \x20 eai update [--check] [--version <tag>] [--force] Self-update `eai` from GitHub Releases (SHA-256)\n\
         \x20 eai doctor [--project <dir>]                     Verify hooks, statusLine, shims, latency & prefix\n\
         \x20 eai pause / eai resume [--project <dir>]         Temporarily pause or resume hook rewriting\n\
         \x20 eai uninstall [--project <dir>] [--global]       Cleanly remove efficient-ai hooks, statusLine & shims\n\
         \n\
         MONITORING & IMPACT PROOF:\n\
         \x20 eai impact [--project <dir>] [--since 30d]       Before vs. After project impact & advisor report\n\
         \x20 eai dashboard [--port 7420] [--since 30d]        Launch local offline web UI (127.0.0.1:7420)\n\
         \x20 eai watch [--project <dir>] [--once]             Live terminal watcher with cache-bust alerts\n\
         \x20 eai share [--out eai-efficiency-card.svg]        Generate privacy-safe SVG efficiency proof card\n\
         \x20 eai stats [--project <dir>] [--global] [--json]  Local savings, recovery rate & baseline metrics\n\
         \x20 eai analyze [path] [--project <dir>] [--since 14d]  Multi-agent token, cache & USD cost profiler\n\
         \x20 eai pricing [--init] [--json]                       View/customize ~/.eai/pricing.json USD rates\n\
         \x20 eai audit [path] [--include-user] [--fix]           Audit & auto-minify static prefix configs\n\
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
        update::print_update_banner_if_available();
        return ExitCode::SUCCESS;
    }

    let sub = args[0].as_str();
    let rest = &args[1..];
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let code = match sub {
        "outline" => {
            let (code, metrics) = outline::run_cli(rest);
            if let Some(m) = metrics {
                telemetry::record_outline(&cwd, m.action, m.raw_chars, m.output_chars);
            }
            return code;
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
            return code;
        }
        "hook" => return ExitCode::from(hooks::cmd_hook(rest) as u8),
        "mcp" => return ExitCode::from(mcp::cmd_mcp(rest) as u8),
        "statusline" => return ExitCode::from(statusline::cmd_statusline(rest) as u8),
        "update" => return ExitCode::from(update::cmd_update(rest) as u8),
        "proxy" => return ExitCode::from(proxy::cmd_proxy(rest) as u8),
        "pause" => setup::cmd_pause(rest, true),
        "resume" => setup::cmd_pause(rest, false),
        "analyze" => tokscope::cmd_analyze(rest),
        "pricing" => pricing::cmd_pricing(rest),
        "audit" => tokscope::cmd_audit(rest),
        "sanitize" => tokscope::cmd_sanitize(rest),
        "selftest" => tokscope::cmd_selftest(),
        "init" | "setup" => setup::cmd_init(rest),
        "doctor" => setup::cmd_doctor(rest),
        "stats" => telemetry::cmd_stats(rest),
        "impact" => impact::cmd_impact(rest),
        "share" => impact::cmd_share(rest),
        "watch" => impact::cmd_watch(rest),
        "dashboard" => dashboard::cmd_dashboard(rest),
        "uninstall" => setup::cmd_uninstall(rest),
        other => {
            eprintln!("Unknown command '{}'. Run `eai --help` for usage.", other);
            return ExitCode::from(2);
        }
    };

    if matches!(sub, "doctor" | "impact" | "analyze" | "stats" | "audit")
        && !rest.iter().any(|a| a == "--json")
    {
        update::print_update_banner_if_available();
    }
    ExitCode::from(code as u8)
}
