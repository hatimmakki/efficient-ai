//! `squeeze`: Lossless-by-default retrievable tool-output compressor, command flag rewriter,
//! and stuck-loop detector for AI coding agents.

pub mod compressor;
pub mod rewriter;

use compressor::compress_stream;
use rewriter::rewrite_command;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_RETAINED_LOGS: usize = 20;

#[derive(Debug, Clone, Default)]
pub struct SqueezeMetrics {
    pub command: String,
    pub raw_chars: usize,
    pub kept_chars: usize,
    pub total_lines: usize,
    pub kept_lines: usize,
    pub elided_lines: usize,
    pub exit_code: i32,
    pub log_path: Option<String>,
    pub loop_detected: bool,
}

pub fn print_usage() {
    eprintln!(
        "squeeze 0.2.0 — Lossless retrievable output compression & loop detection\n\
         \n\
         USAGE:\n\
         \x20 <command> | squeeze [--max-lines N]     Compress piped stdout; spill full log to .eai/logs/\n\
         \x20 squeeze [--max-lines N] -- <cmd> [args] Run command, compress stdout+stderr, detect stuck loops\n\
         \x20 squeeze --rewrite \"<command>\"           Print command rewritten with compact H8 flags\n\
         \x20 squeeze --install-shims <dir>           Create portable executable PATH shims in <dir>\n"
    );
}

pub fn run_cli(args: &[String]) -> (ExitCode, Option<SqueezeMetrics>) {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_usage();
        return (ExitCode::SUCCESS, None);
    }

    let mut max_lines: usize = 150;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--rewrite" if i + 1 < args.len() => {
                println!("{}", rewrite_command(&args[i + 1]));
                return (ExitCode::SUCCESS, None);
            }
            "--install-shims" if i + 1 < args.len() => {
                let dir = PathBuf::from(&args[i + 1]);
                let exe = env::current_exe().unwrap_or_else(|_| PathBuf::from("squeeze"));
                return match install_shims_for_exe(&dir, &exe, false) {
                    Ok(n) => {
                        println!("Installed {} portable PATH shims in {}", n, dir.display());
                        (ExitCode::SUCCESS, None)
                    }
                    Err(e) => {
                        eprintln!("Error installing shims: {}", e);
                        (ExitCode::from(1), None)
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
                return run_wrapped_command(&args[i..], max_lines);
            }
        }
    }

    // No command args: read from stdin
    let mut raw = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut raw) {
        eprintln!("Error reading stdin: {}", e);
        return (ExitCode::from(1), None);
    }
    let metrics = emit_compressed("stdin", &raw, max_lines, 0, None);
    (ExitCode::SUCCESS, Some(metrics))
}

/// Spawn an external program reliably on both Unix and Windows (handling `.cmd`/`.bat` like `npm` and `pnpm`).
fn spawn_command_in(prog: &str, rest: &[String], cwd: Option<&Path>) -> io::Result<Output> {
    #[cfg(windows)]
    {
        let mut c = Command::new(prog);
        c.args(rest);
        if let Some(d) = cwd {
            c.current_dir(d);
        }
        match c.output() {
            Ok(out) => Ok(out),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let mut cmd = Command::new("cmd");
                cmd.arg("/C").arg(prog).args(rest);
                if let Some(d) = cwd {
                    cmd.current_dir(d);
                }
                cmd.output()
            }
            Err(e) => Err(e),
        }
    }
    #[cfg(not(windows))]
    {
        let mut c = Command::new(prog);
        c.args(rest);
        if let Some(d) = cwd {
            c.current_dir(d);
        }
        c.output()
    }
}

/// Run a shell command string (via `sh -c` on Unix or `cmd /C` on Windows), compress output
/// losslessly, spill full log to `<cwd>/.eai/logs/`, and return `(exit_code, rendered_text, metrics)`
/// without writing anything to stdout (safe for MCP stdio servers).
pub fn run_shell_capture(
    shell_cmd: &str,
    cwd: &Path,
    max_lines: usize,
) -> (i32, String, SqueezeMetrics) {
    let output = {
        #[cfg(windows)]
        {
            Command::new("cmd")
                .args(["/C", shell_cmd])
                .current_dir(cwd)
                .output()
        }
        #[cfg(not(windows))]
        {
            Command::new("sh")
                .args(["-c", shell_cmd])
                .current_dir(cwd)
                .output()
        }
    };

    let out = match output {
        Ok(o) => o,
        Err(e) => {
            let msg = format!("squeeze: failed to execute '{}': {}", shell_cmd, e);
            return (
                127,
                msg,
                SqueezeMetrics {
                    command: shell_cmd.to_string(),
                    exit_code: 127,
                    ..Default::default()
                },
            );
        }
    };

    let stdout_s = String::from_utf8_lossy(&out.stdout);
    let stderr_s = String::from_utf8_lossy(&out.stderr);
    let combined = if stderr_s.is_empty() {
        stdout_s.into_owned()
    } else if stdout_s.is_empty() {
        stderr_s.into_owned()
    } else {
        format!("{}\n{}", stdout_s.trim_end(), stderr_s)
    };

    let code = out.status.code().unwrap_or(1);
    let state_dir = cwd.join(".eai");
    let loop_nudge = record_and_check_loop_in(&state_dir, shell_cmd, code, &combined);
    let (rendered, metrics) =
        format_compressed_in(&state_dir.join("logs"), shell_cmd, &combined, max_lines, code, loop_nudge.as_deref());
    (code, rendered, metrics)
}

pub fn run_wrapped_command(cmd_args: &[String], max_lines: usize) -> (ExitCode, Option<SqueezeMetrics>) {
    if cmd_args.is_empty() {
        print_usage();
        return (ExitCode::from(2), None);
    }

    let prog = &cmd_args[0];
    let rest = &cmd_args[1..];

    let output = match spawn_command_in(prog, rest, None) {
        Ok(out) => out,
        Err(e) => {
            eprintln!("squeeze: failed to execute '{}': {}", prog, e);
            return (ExitCode::from(127), None);
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

    let metrics = emit_compressed(&cmd_str, &combined, max_lines, code, loop_nudge.as_deref());
    (ExitCode::from(code as u8), Some(metrics))
}

fn format_compressed_in(
    log_dir: &Path,
    cmd_label: &str,
    raw: &str,
    max_lines: usize,
    exit_code: i32,
    loop_nudge: Option<&str>,
) -> (String, SqueezeMetrics) {
    let res = compress_stream(raw, max_lines);
    let mut rendered = String::new();
    if !res.output.is_empty() {
        rendered.push_str(&res.output);
        rendered.push('\n');
    }
    let mut spilled_path: Option<String> = None;
    if res.elided_lines > 0 {
        if let Ok(log_path) = spill_to_log_in(log_dir, raw) {
            let p_str = log_path.display().to_string();
            rendered.push_str(&format!(
                "[{} passing/duplicate lines elided ({} -> {} lines); full log: {}]\n",
                res.elided_lines, res.total_lines, res.kept_lines, p_str
            ));
            spilled_path = Some(p_str);
        } else {
            rendered.push_str(&format!(
                "[{} passing/duplicate lines elided ({} -> {} lines)]\n",
                res.elided_lines, res.total_lines, res.kept_lines
            ));
        }
    }
    if let Some(nudge) = loop_nudge {
        rendered.push_str(nudge);
        rendered.push('\n');
    }

    let metrics = SqueezeMetrics {
        command: cmd_label.to_string(),
        raw_chars: raw.len(),
        kept_chars: res.output.len(),
        total_lines: res.total_lines,
        kept_lines: res.kept_lines,
        elided_lines: res.elided_lines,
        exit_code,
        log_path: spilled_path,
        loop_detected: loop_nudge.is_some(),
    };
    (rendered, metrics)
}

fn emit_compressed(
    cmd_label: &str,
    raw: &str,
    max_lines: usize,
    exit_code: i32,
    loop_nudge: Option<&str>,
) -> SqueezeMetrics {
    let (rendered, metrics) = format_compressed_in(
        &PathBuf::from(".eai").join("logs"),
        cmd_label,
        raw,
        max_lines,
        exit_code,
        loop_nudge,
    );
    print!("{}", rendered);
    metrics
}

/// Write raw log to a concurrency-safe unique path `.eai/logs/<ms>-<pid>.log`, refresh `last.log`,
/// and prune older logs beyond `MAX_RETAINED_LOGS`.
pub fn spill_to_log(raw: &str) -> io::Result<PathBuf> {
    spill_to_log_in(&PathBuf::from(".eai").join("logs"), raw)
}

pub fn spill_to_log_in(log_dir: &Path, raw: &str) -> io::Result<PathBuf> {
    fs::create_dir_all(log_dir)?;
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let pid = std::process::id();
    let unique_path = log_dir.join(format!("{}-{}.log", ms, pid));
    fs::write(&unique_path, raw)?;
    // Maintain `last.log` convenience pointer for backwards compatibility
    let _ = fs::write(log_dir.join("last.log"), raw);
    prune_old_logs(log_dir, MAX_RETAINED_LOGS);
    Ok(unique_path)
}

fn prune_old_logs(log_dir: &Path, keep: usize) {
    let Ok(entries) = fs::read_dir(log_dir) else {
        return;
    };
    let mut logs: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension().and_then(|x| x.to_str()) == Some("log")
                && p.file_name().and_then(|x| x.to_str()) != Some("last.log")
        })
        .collect();
    if logs.len() <= keep {
        return;
    }
    logs.sort();
    for old in logs.iter().take(logs.len() - keep) {
        let _ = fs::remove_file(old);
    }
}

/// Deterministic FNV-1a 64-bit hash (stable across Rust builds & runs).
fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Normalize volatile tokens (digits, timestamps, durations, hex IDs) so identical failures
/// with different timestamps or PIDs produce the same loop fingerprint.
pub fn normalize_for_fingerprint(output: &str) -> String {
    let compressed = compress_stream(output, 60).output;
    let mut norm = String::with_capacity(compressed.len().min(600));
    let mut prev_digit = false;
    for ch in compressed.chars().take(600) {
        if ch.is_ascii_digit() {
            if !prev_digit {
                norm.push('#');
                prev_digit = true;
            }
        } else {
            prev_digit = false;
            norm.push(ch);
        }
    }
    norm
}

/// H7 Stuck-loop detector: tracks consecutive identical failing command + normalized error hashes
/// in `.eai/loop_state.txt`.
pub fn record_and_check_loop(cmd: &str, exit_code: i32, output: &str) -> Option<String> {
    record_and_check_loop_in(&PathBuf::from(".eai"), cmd, exit_code, output)
}

pub fn record_and_check_loop_in(
    state_dir: &Path,
    cmd: &str,
    exit_code: i32,
    output: &str,
) -> Option<String> {
    let state_file = state_dir.join("loop_state.txt");

    if exit_code == 0 {
        let _ = fs::remove_file(&state_file);
        return None;
    }

    let norm_err = normalize_for_fingerprint(output);
    let sig = format!("{}:{}:{:016x}", cmd, exit_code, fnv1a_64(norm_err.as_bytes()));

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
    if fs::create_dir_all(state_dir).is_ok() {
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

/// Install portable executable PATH shims.
/// When `use_eai_subcommand` is true, shims invoke `"<exe>" squeeze -- <prog> "$@"`.
/// Otherwise, shims invoke `"<exe>" -- <prog> "$@"`.
pub fn install_shims_for_exe(
    dir: &Path,
    exe: &Path,
    use_eai_subcommand: bool,
) -> io::Result<usize> {
    fs::create_dir_all(dir)?;
    let exe_str = exe.display().to_string();
    let sub = if use_eai_subcommand { "squeeze --" } else { "--" };
    let programs = ["pytest", "cargo", "npm", "pnpm", "yarn", "go"];
    let mut count = 0;

    for prog in programs {
        // Unix shell shim
        let sh_path = dir.join(prog);
        let sh_content = format!(
            "#!/usr/bin/env sh\n\
             # Portable efficient-ai PATH shim for {prog}\n\
             SHIM_DIR=\"$(cd \"$(dirname \"$0\")\" && pwd)\"\n\
             PATH=\"$(echo \"$PATH\" | sed -e \"s|$SHIM_DIR:||g\" -e \"s|:$SHIM_DIR||g\")\"\n\
             export PATH\n\
             exec \"{exe_str}\" {sub} \"{prog}\" \"$@\"\n"
        );
        fs::write(&sh_path, sh_content)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&sh_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&sh_path, perms)?;
        }

        // Windows .cmd shim
        let cmd_path = dir.join(format!("{}.cmd", prog));
        let cmd_content = format!(
            "@echo off\r\n\
             setlocal\r\n\
             set \"SHIM_DIR=%~dp0\"\r\n\
             set \"PATH=%PATH:%SHIM_DIR%;=%\"\r\n\
             \"{exe_str}\" {sub} {prog} %*\r\n"
        );
        fs::write(&cmd_path, cmd_content)?;
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loop_detector_normalizes_timestamps() {
        let tmp = env::temp_dir().join(format!("eai_loop_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);

        let err1 = "2026-11-24 09:00:01 ERROR AssertionError: expected 42 in 0.12s";
        let err2 = "2026-11-24 09:00:08 ERROR AssertionError: expected 42 in 0.19s";
        let err3 = "2026-11-24 09:00:15 ERROR AssertionError: expected 42 in 0.24s";

        assert!(record_and_check_loop_in(&tmp, "pytest", 1, err1).is_none());
        assert!(record_and_check_loop_in(&tmp, "pytest", 1, err2).is_none());
        let nudge = record_and_check_loop_in(&tmp, "pytest", 1, err3);
        assert!(nudge.is_some());
        assert!(nudge.unwrap().contains("3x identical failure"));

        // Passing run resets state
        assert!(record_and_check_loop_in(&tmp, "pytest", 0, "ok").is_none());
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_spill_to_unique_log_and_prune() {
        let tmp = env::temp_dir().join(format!("eai_log_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);

        let p = spill_to_log_in(&tmp, "full raw output\nline 2").expect("spill");
        assert!(p.is_file());
        assert!(tmp.join("last.log").is_file());
        assert_eq!(fs::read_to_string(&p).unwrap(), "full raw output\nline 2");

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_install_shims_creates_unix_and_cmd() {
        let tmp = env::temp_dir().join(format!("eai_shim_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);

        let count = install_shims_for_exe(&tmp, Path::new("/usr/local/bin/eai"), true)
            .expect("install shims");
        assert_eq!(count, 6);
        let pytest_sh = fs::read_to_string(tmp.join("pytest")).unwrap();
        assert!(pytest_sh.contains("squeeze -- \"pytest\""));
        let npm_cmd = fs::read_to_string(tmp.join("npm.cmd")).unwrap();
        assert!(npm_cmd.contains("squeeze -- npm"));

        let _ = fs::remove_dir_all(&tmp);
    }
}
