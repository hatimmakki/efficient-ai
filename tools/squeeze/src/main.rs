//! `squeeze`: Lossless-by-default retrievable tool-output compressor, command flag rewriter,
//! and stuck-loop detector for AI coding agents.
//!
//! Usage:
//!   <cmd> | squeeze [--max-lines N]               # Compress stdin, spill raw to .eai/logs/last.log
//!   squeeze -- <program> [args...]                # Execute program, compress output, detect loops
//!   squeeze --rewrite "<shell_command>"           # Rewrite command with H8 compact flags
//!   squeeze --install-shims <dir>                 # Generate portable PATH shims for git/pytest/cargo

mod compressor;
mod rewriter;

use compressor::compress_stream;
use rewriter::rewrite_command;
use std::collections::hash_map::DefaultHasher;
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn print_usage() {
    eprintln!(
        "squeeze 0.1.0 — Lossless retrievable output compression & loop detection\n\
         \n\
         USAGE:\n\
         \x20 <command> | squeeze [--max-lines N]     Compress piped stdout; spill full log to .eai/logs/last.log\n\
         \x20 squeeze [--max-lines N] -- <cmd> [args] Run command, compress stdout+stderr, detect stuck loops\n\
         \x20 squeeze --rewrite \"<command>\"           Print command rewritten with compact H8 flags\n\
         \x20 squeeze --install-shims <dir>           Create portable PATH shims in <dir>\n"
    );
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_usage();
        return ExitCode::SUCCESS;
    }

    let mut max_lines: usize = 150;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--rewrite" if i + 1 < args.len() => {
                println!("{}", rewrite_command(&args[i + 1]));
                return ExitCode::SUCCESS;
            }
            "--install-shims" if i + 1 < args.len() => {
                let dir = PathBuf::from(&args[i + 1]);
                return match install_shims(&dir) {
                    Ok(n) => {
                        println!("Installed {} portable PATH shims in {}", n, dir.display());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("Error installing shims: {}", e);
                        ExitCode::from(1)
                    }
                };
            }
            "--max-lines" | "-m" if i + 1 < args.len() => {
                max_lines = args[i + 1].parse().unwrap_or(150);
                i += 2;
            }
            "--" => {
                return run_wrapped_command(&args[i + 1..], max_lines);
            }
            _ => {
                // Treat remaining args as wrapped command
                return run_wrapped_command(&args[i..], max_lines);
            }
        }
    }

    // No command args: read from stdin
    let mut raw = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut raw) {
        eprintln!("Error reading stdin: {}", e);
        return ExitCode::from(1);
    }
    emit_compressed(&raw, max_lines, None);
    ExitCode::SUCCESS
}

fn run_wrapped_command(cmd_args: &[String], max_lines: usize) -> ExitCode {
    if cmd_args.is_empty() {
        print_usage();
        return ExitCode::from(2);
    }

    let prog = &cmd_args[0];
    let rest = &cmd_args[1..];

    let output = match Command::new(prog).args(rest).output() {
        Ok(out) => out,
        Err(e) => {
            eprintln!("squeeze: failed to execute '{}': {}", prog, e);
            return ExitCode::from(127);
        }
    };

    let stdout_s = String::from_utf8_lossy(&output.stdout);
    let stderr_s = String::from_utf8_lossy(&output.stderr);
    let combined = if stderr_s.is_empty() {
        stdout_s.into_owned()
    } else if stdout_s.is_empty() {
        stderr_s.into_owned()
    } else {
        format!("{}\n{}", stdout_s.trim_end(), stderr_s)
    };

    let code = output.status.code().unwrap_or(1);
    let cmd_str = cmd_args.join(" ");
    let loop_nudge = record_and_check_loop(&cmd_str, code, &combined);

    emit_compressed(&combined, max_lines, loop_nudge.as_deref());
    ExitCode::from(code as u8)
}

fn emit_compressed(raw: &str, max_lines: usize, loop_nudge: Option<&str>) {
    let res = compress_stream(raw, max_lines);
    if !res.output.is_empty() {
        println!("{}", res.output);
    }
    if res.elided_lines > 0 {
        if let Ok(log_path) = spill_to_log(raw) {
            println!(
                "[{} passing/duplicate lines elided ({} -> {} lines); full log: {}]",
                res.elided_lines,
                res.total_lines,
                res.kept_lines,
                log_path.display()
            );
        } else {
            println!(
                "[{} passing/duplicate lines elided ({} -> {} lines)]",
                res.elided_lines, res.total_lines, res.kept_lines
            );
        }
    }
    if let Some(nudge) = loop_nudge {
        println!("{}", nudge);
    }
}

fn spill_to_log(raw: &str) -> io::Result<PathBuf> {
    let log_dir = PathBuf::from(".eai").join("logs");
    fs::create_dir_all(&log_dir)?;
    let last_path = log_dir.join("last.log");
    fs::write(&last_path, raw)?;
    Ok(last_path)
}

fn hash_signature(s: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// H7 Stuck-loop detector: tracks consecutive identical failing command + output hashes in `.eai/loop_state.txt`.
fn record_and_check_loop(cmd: &str, exit_code: i32, output: &str) -> Option<String> {
    let state_dir = PathBuf::from(".eai");
    let state_file = state_dir.join("loop_state.txt");

    if exit_code == 0 {
        let _ = fs::remove_file(&state_file);
        return None;
    }

    // Fingerprint command + exit code + first 400 chars of cleaned error output
    let err_head: String = output.chars().take(400).collect();
    let sig = format!("{}:{}:{:x}", cmd, exit_code, hash_signature(&err_head));

    let (prev_sig, prev_count) = fs::read_to_string(&state_file)
        .ok()
        .and_then(|s| {
            let mut lines = s.lines();
            let ps = lines.next()?.to_string();
            let pc: usize = lines.next()?.parse().ok()?;
            Some((ps, pc))
        })
        .unwrap_or_else(|| (String::new(), 0));

    let new_count = if prev_sig == sig { prev_count + 1 } else { 1 };
    if fs::create_dir_all(&state_dir).is_ok() {
        let _ = fs::write(&state_file, format!("{}\n{}\n", sig, new_count));
    }

    if new_count >= 3 {
        Some(format!(
            "[LOOP DETECTED ({}x identical failure for '{}'): stop repeating this command; inspect .eai/logs/last.log or change strategy]",
            new_count, cmd
        ))
    } else {
        None
    }
}

fn install_shims(dir: &Path) -> io::Result<usize> {
    fs::create_dir_all(dir)?;
    let exe = env::current_exe().unwrap_or_else(|_| PathBuf::from("squeeze"));
    let exe_str = exe.display().to_string();
    let programs = ["pytest", "cargo", "npm"];
    let mut count = 0;

    for prog in programs {
        // Unix shell shim
        let sh_path = dir.join(prog);
        let sh_content = format!(
            "#!/usr/bin/env sh\n\
             # Portable squeeze PATH shim for {prog}\n\
             SHIM_DIR=\"$(cd \"$(dirname \"$0\")\" && pwd)\"\n\
             PATH=\"$(echo \"$PATH\" | sed -e \"s|$SHIM_DIR:||g\" -e \"s|:$SHIM_DIR||g\")\"\n\
             export PATH\n\
             exec \"{exe_str}\" -- \"{prog}\" \"$@\"\n"
        );
        fs::write(&sh_path, sh_content)?;

        // Windows .cmd shim
        let cmd_path = dir.join(format!("{}.cmd", prog));
        let cmd_content = format!(
            "@echo off\r\n\
             setlocal\r\n\
             set \"SHIM_DIR=%~dp0\"\r\n\
             set \"PATH=%PATH:%SHIM_DIR%;=%\"\r\n\
             \"{exe_str}\" -- {prog} %*\r\n"
        );
        fs::write(&cmd_path, cmd_content)?;
        count += 1;
    }
    Ok(count)
}
