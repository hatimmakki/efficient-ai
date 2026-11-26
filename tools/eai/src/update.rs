//! Self-updater (`eai update`) and non-blocking 24-hour release checker.
//!
//! Design invariants:
//! - Hot paths (`hook`, `statusline`, `outline`, `squeeze`, `mcp`) NEVER block on network.
//! - Update availability is read from `~/.eai/update-check.json` (<0.2ms local file read).
//! - Background refresh spawns a detached `eai update --refresh-cache` at most once per 24h.
//! - Opt-out via `EAI_NO_UPDATE_CHECK=1` or `DO_NOT_TRACK=1`.

use crate::telemetry::{now_unix_secs, user_home};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

pub const REPO: &str = "hatimmakki/efficient-ai";
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const CACHE_TTL_SECS: u64 = 24 * 3600;
const RECEIPT_NOTICE_TTL_SECS: u64 = 24 * 3600;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCache {
    pub checked_at: u64,
    pub latest_version: String,
    pub release_url: String,
}

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub release_url: String,
}

pub fn update_check_disabled() -> bool {
    for key in ["EAI_NO_UPDATE_CHECK", "DO_NOT_TRACK"] {
        if let Ok(v) = env::var(key) {
            let t = v.trim();
            if !t.is_empty() && t != "0" && !t.eq_ignore_ascii_case("false") {
                return true;
            }
        }
    }
    false
}

pub fn parse_semver(raw: &str) -> Option<(u64, u64, u64)> {
    let trimmed = raw.trim().trim_start_matches('v').trim_start_matches('V');
    let core = trimmed.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    Some((major, minor, patch))
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_semver(latest), parse_semver(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

pub fn cache_path_in(home: &Path) -> PathBuf {
    home.join(".eai").join("update-check.json")
}

fn cache_path() -> Option<PathBuf> {
    user_home().map(|h| cache_path_in(&h))
}

pub fn read_cache_from(path: &Path) -> Option<UpdateCache> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str::<UpdateCache>(&raw).ok()
}

pub fn write_cache_to(path: &Path, cache: &UpdateCache) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_string_pretty(cache)?;
    fs::write(path, format!("{}\n", body))
}

/// Fast local-only check (<0.2ms) returning `Some(UpdateInfo)` when a newer version is cached.
pub fn cached_available_update_in(home: &Path, current_ver: &str) -> Option<UpdateInfo> {
    if update_check_disabled() {
        return None;
    }
    let cache = read_cache_from(&cache_path_in(home))?;
    let clean_latest = cache.latest_version.trim().trim_start_matches('v').to_string();
    let clean_current = current_ver.trim().trim_start_matches('v').to_string();
    if is_newer(&clean_latest, &clean_current) {
        Some(UpdateInfo {
            current: clean_current,
            latest: clean_latest,
            release_url: cache.release_url,
        })
    } else {
        None
    }
}

pub fn cached_available_update() -> Option<UpdateInfo> {
    let h = user_home()?;
    cached_available_update_in(&h, CURRENT_VERSION)
}

/// Spawn a detached silent `eai update --refresh-cache` if cache is missing or older than 24h.
/// Never blocks the caller and never flashes a console window.
pub fn maybe_spawn_background_check() {
    if update_check_disabled() {
        return;
    }
    let Some(path) = cache_path() else {
        return;
    };
    let now = now_unix_secs();
    if let Some(c) = read_cache_from(&path) {
        if now.saturating_sub(c.checked_at) < CACHE_TTL_SECS {
            return;
        }
    }
    // Stamp lock timestamp first so rapid consecutive hook/statusline ticks don't spawn storms.
    let existing_ver = read_cache_from(&path)
        .map(|c| c.latest_version)
        .unwrap_or_else(|| CURRENT_VERSION.to_string());
    let existing_url = read_cache_from(&path)
        .map(|c| c.release_url)
        .unwrap_or_else(|| format!("https://github.com/{}/releases/latest", REPO));
    let _ = write_cache_to(
        &path,
        &UpdateCache {
            checked_at: now,
            latest_version: existing_ver,
            release_url: existing_url,
        },
    );

    let Ok(exe) = env::current_exe() else {
        return;
    };
    let mut cmd = Command::new(exe);
    cmd.args(["update", "--refresh-cache"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd.spawn();
}

/// Print a concise 1-line update notice to stderr for interactive CLI commands.
pub fn print_update_banner_if_available() {
    maybe_spawn_background_check();
    if let Some(info) = cached_available_update() {
        eprintln!(
            "\n[eai] Update available: v{} -> v{} — run `eai update` ({})",
            info.current, info.latest, info.release_url
        );
    }
}

/// Once-per-24h zero-LLM-token markdown suffix for Antigravity `PostInvocation` receipts.
pub fn antigravity_receipt_update_suffix() -> Option<String> {
    maybe_spawn_background_check();
    let info = cached_available_update()?;
    let h = user_home()?;
    let marker = h.join(".eai").join(".last_update_notice");
    let now = now_unix_secs();
    let last_shown: u64 = fs::read_to_string(&marker)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    if now.saturating_sub(last_shown) < RECEIPT_NOTICE_TTL_SECS {
        return None;
    }
    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&marker, format!("{}\n", now));
    Some(format!(
        " · **Update:** v{} → v{} (`eai update`)",
        info.current, info.latest
    ))
}

fn http_agent(timeout_secs: u64) -> ureq::Agent {
    let cfg = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(timeout_secs)))
        .build();
    ureq::Agent::new_with_config(cfg)
}

fn fetch_latest_release() -> Result<(String, String), String> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", REPO);
    let agent = http_agent(8);
    let resp = agent
        .get(&url)
        .header("User-Agent", &format!("eai/{}", CURRENT_VERSION))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("GitHub API request failed: {}", e))?;

    let mut body = String::new();
    resp.into_body()
        .into_reader()
        .read_to_string(&mut body)
        .map_err(|e| format!("Failed reading release JSON: {}", e))?;

    let val: Value = serde_json::from_str(&body)
        .map_err(|e| format!("Invalid GitHub release JSON: {}", e))?;
    let tag = val
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing tag_name in GitHub release".to_string())?
        .to_string();
    let html_url = val
        .get("html_url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("https://github.com/{}/releases/tag/{}", REPO, tag));
    Ok((tag, html_url))
}

fn platform_target() -> Result<(&'static str, &'static str, &'static str), String> {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        Ok(("x86_64-pc-windows-msvc", "zip", "eai.exe"))
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        Ok(("aarch64-apple-darwin", "tar.gz", "eai"))
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        Ok(("x86_64-apple-darwin", "tar.gz", "eai"))
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        Ok(("x86_64-unknown-linux-gnu", "tar.gz", "eai"))
    }
    #[cfg(not(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
    )))]
    {
        Err(format!(
            "No prebuilt binary for this OS/arch. Update from source with: cargo install --git https://github.com/{}.git eai --force",
            REPO
        ))
    }
}

/// Pure-Rust FIPS 180-4 SHA-256 (zero external crate or shell dependency).
pub fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    let (chunks, _) = msg.as_chunks::<64>();
    for chunk in chunks {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[4 * i],
                chunk[4 * i + 1],
                chunk[4 * i + 2],
                chunk[4 * i + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = String::with_capacity(64);
    for word in h {
        out.push_str(&format!("{:08x}", word));
    }
    out
}

pub fn parse_expected_sha256(sums_text: &str, archive_name: &str) -> Option<String> {
    for line in sums_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let hash = parts.next()?;
        let fname = parts.next()?.trim_start_matches('*');
        if (fname == archive_name || fname.ends_with(&format!("/{}", archive_name)))
            && hash.len() == 64
            && hash.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Some(hash.to_ascii_lowercase());
        }
    }
    None
}

fn download_bytes(url: &str) -> Result<Vec<u8>, String> {
    let agent = http_agent(45);
    let resp = agent
        .get(url)
        .header("User-Agent", &format!("eai/{}", CURRENT_VERSION))
        .call()
        .map_err(|e| format!("Download failed ({}): {}", url, e))?;
    let mut bytes = Vec::new();
    resp.into_body()
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Read failed ({}): {}", url, e))?;
    Ok(bytes)
}

fn extract_binary(archive_path: &Path, dest_dir: &Path, ext: &str, bin_name: &str) -> Result<PathBuf, String> {
    let status = if ext == "zip" {
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &format!(
                    "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
                    archive_path.display().to_string().replace('\'', "''"),
                    dest_dir.display().to_string().replace('\'', "''"),
                ),
            ])
            .status()
            .map_err(|e| format!("Failed to invoke powershell Expand-Archive: {}", e))?
    } else {
        Command::new("tar")
            .args(["-xzf"])
            .arg(archive_path)
            .arg("-C")
            .arg(dest_dir)
            .status()
            .map_err(|e| format!("Failed to invoke tar: {}", e))?
    };

    if !status.success() {
        return Err(format!("Archive extraction exited with {}", status));
    }
    let extracted = dest_dir.join(bin_name);
    if extracted.is_file() {
        return Ok(extracted);
    }
    // Search one level deep if archive contained a wrapper folder
    if let Ok(entries) = fs::read_dir(dest_dir) {
        for entry in entries.flatten() {
            let candidate = entry.path().join(bin_name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(format!("Extracted archive did not contain `{}`", bin_name))
}

fn replace_current_executable(new_bin: &Path, target_exe: &Path) -> Result<(), String> {
    let parent = target_exe
        .parent()
        .ok_or_else(|| "Invalid executable path".to_string())?;
    let staged = parent.join(format!(
        "{}.new",
        target_exe
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("eai")
    ));
    let backup = parent.join(format!(
        "{}.old",
        target_exe
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("eai")
    ));

    // Clean up previous backup if unlocked
    if backup.exists() {
        let _ = fs::remove_file(&backup);
    }

    fs::copy(new_bin, &staged)
        .map_err(|e| format!("Failed staging new binary to {}: {}", staged.display(), e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&staged, fs::Permissions::from_mode(0o755));
        fs::rename(&staged, target_exe)
            .map_err(|e| format!("Failed replacing {}: {}", target_exe.display(), e))?;
    }

    #[cfg(windows)]
    {
        // Windows locks overwriting a running .exe, but allows renaming it first.
        fs::rename(target_exe, &backup)
            .map_err(|e| format!("Failed moving current exe to {}: {}", backup.display(), e))?;
        if let Err(e) = fs::rename(&staged, target_exe) {
            // Roll back on failure
            let _ = fs::rename(&backup, target_exe);
            return Err(format!("Failed moving new binary into place: {}", e));
        }
    }

    Ok(())
}

pub fn cmd_update(args: &[String]) -> i32 {
    let mut check_only = false;
    let mut refresh_cache = false;
    let mut force = false;
    let mut pinned_version: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--check" => check_only = true,
            "--refresh-cache" => refresh_cache = true,
            "--force" | "-f" => force = true,
            "--version" if i + 1 < args.len() => {
                pinned_version = Some(args[i + 1].clone());
                i += 1;
            }
            "-h" | "--help" => {
                println!(
                    "Usage: eai update [--check] [--version <vX.Y.Z>] [--force]\n\
                     \n\
                     Checks GitHub Releases for efficient-ai and safely upgrades the `eai` binary in place.\n\
                     Set EAI_NO_UPDATE_CHECK=1 to disable passive 24h update notices."
                );
                return 0;
            }
            _ => {}
        }
        i += 1;
    }

    // Best-effort cleanup of any leftover `.old` Windows binary from a prior self-update
    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            let old = parent.join(format!(
                "{}.old",
                exe.file_name().and_then(|n| n.to_str()).unwrap_or("eai")
            ));
            if old.is_file() {
                let _ = fs::remove_file(&old);
            }
        }
    }

    if refresh_cache {
        if update_check_disabled() {
            return 0;
        }
        if let Ok((tag, html_url)) = fetch_latest_release() {
            if let Some(cp) = cache_path() {
                let _ = write_cache_to(
                    &cp,
                    &UpdateCache {
                        checked_at: now_unix_secs(),
                        latest_version: tag.trim_start_matches('v').to_string(),
                        release_url: html_url,
                    },
                );
            }
        }
        return 0;
    }

    let (target_tag, release_url) = if let Some(v) = pinned_version {
        let tag = if v.starts_with('v') { v } else { format!("v{}", v) };
        let url = format!("https://github.com/{}/releases/tag/{}", REPO, tag);
        (tag, url)
    } else {
        match fetch_latest_release() {
            Ok(pair) => {
                if let Some(cp) = cache_path() {
                    let _ = write_cache_to(
                        &cp,
                        &UpdateCache {
                            checked_at: now_unix_secs(),
                            latest_version: pair.0.trim_start_matches('v').to_string(),
                            release_url: pair.1.clone(),
                        },
                    );
                }
                pair
            }
            Err(e) => {
                eprintln!("Error checking latest release: {}", e);
                return 1;
            }
        }
    };

    let clean_target = target_tag.trim_start_matches('v');
    let newer = is_newer(clean_target, CURRENT_VERSION);

    if check_only {
        if newer {
            println!(
                "Update available: eai v{} -> v{}\nRelease: {}\nRun `eai update` to upgrade.",
                CURRENT_VERSION, clean_target, release_url
            );
        } else {
            println!("eai v{} is up to date (latest: v{}).", CURRENT_VERSION, clean_target);
        }
        return 0;
    }

    if !newer && !force {
        println!(
            "eai v{} is already up to date (latest: v{}). Pass `--force` to reinstall.",
            CURRENT_VERSION, clean_target
        );
        return 0;
    }

    let (triple, ext, bin_name) = match platform_target() {
        Ok(t) => t,
        Err(msg) => {
            eprintln!("{}", msg);
            return 1;
        }
    };

    let archive_name = format!("eai-{}.{}", triple, ext);
    let base_dl = format!("https://github.com/{}/releases/download/{}", REPO, target_tag);
    let archive_url = format!("{}/{}", base_dl, archive_name);
    let sums_url = format!("{}/SHA256SUMS.txt", base_dl);

    println!(
        "Updating eai v{} -> v{} ({})...",
        CURRENT_VERSION, clean_target, triple
    );
    let archive_bytes = match download_bytes(&archive_url) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Error: {}", e);
            return 1;
        }
    };

    // Verify SHA-256 against SHA256SUMS.txt when published alongside the release
    if let Ok(sums_bytes) = download_bytes(&sums_url) {
        if let Ok(sums_str) = String::from_utf8(sums_bytes) {
            if let Some(expected) = parse_expected_sha256(&sums_str, &archive_name) {
                let actual = sha256_hex(&archive_bytes);
                if actual != expected {
                    eprintln!(
                        "Error: SHA-256 checksum mismatch for {}!\n  expected: {}\n  actual:   {}",
                        archive_name, expected, actual
                    );
                    return 1;
                }
                println!("- [OK] SHA-256 verified ({}...)", &actual[..12]);
            }
        }
    }

    let tmp_dir = env::temp_dir().join(format!("eai-update-{}", now_unix_secs()));
    let _ = fs::remove_dir_all(&tmp_dir);
    if let Err(e) = fs::create_dir_all(&tmp_dir) {
        eprintln!("Error creating temp dir: {}", e);
        return 1;
    }
    let archive_path = tmp_dir.join(&archive_name);
    if let Err(e) = fs::write(&archive_path, &archive_bytes) {
        eprintln!("Error writing downloaded archive: {}", e);
        let _ = fs::remove_dir_all(&tmp_dir);
        return 1;
    }

    let new_bin = match extract_binary(&archive_path, &tmp_dir, ext, bin_name) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error extracting update: {}", e);
            let _ = fs::remove_dir_all(&tmp_dir);
            return 1;
        }
    };

    let target_exe = match env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error resolving current executable path: {}", e);
            let _ = fs::remove_dir_all(&tmp_dir);
            return 1;
        }
    };

    if let Err(e) = replace_current_executable(&new_bin, &target_exe) {
        eprintln!("Error installing update: {}", e);
        let _ = fs::remove_dir_all(&tmp_dir);
        return 1;
    }
    let _ = fs::remove_dir_all(&tmp_dir);

    if let Some(cp) = cache_path() {
        let _ = write_cache_to(
            &cp,
            &UpdateCache {
                checked_at: now_unix_secs(),
                latest_version: clean_target.to_string(),
                release_url: release_url.clone(),
            },
        );
    }

    println!("- [OK] Updated `{}` to eai v{}", target_exe.display(), clean_target);
    println!("- Release notes: {}", release_url);
    println!("\nIf you use native plugins, refresh them too:");
    println!("  Antigravity: agy plugin install https://github.com/{}", REPO);
    println!("  Claude Code: /plugin update efficient-ai@efficient-ai");
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semver_comparison() {
        assert!(is_newer("1.2.0", "1.1.0"));
        assert!(is_newer("v2.0.0", "1.9.9"));
        assert!(is_newer("1.1.1", "1.1.0"));
        assert!(!is_newer("1.1.0", "1.1.0"));
        assert!(!is_newer("1.0.9", "1.1.0"));
        assert!(!is_newer("invalid", "1.1.0"));
    }

    #[test]
    fn test_sha256_known_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn test_parse_sha256sums_line() {
        let sample = "\
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  eai-x86_64-pc-windows-msvc.zip
ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  eai-aarch64-apple-darwin.tar.gz
";
        assert_eq!(
            parse_expected_sha256(sample, "eai-x86_64-pc-windows-msvc.zip").as_deref(),
            Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        );
        assert!(parse_expected_sha256(sample, "missing.zip").is_none());
    }

    #[test]
    fn test_cached_update_detection() {
        let tmp = env::temp_dir().join("eai_test_update_cache");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let cp = cache_path_in(&tmp);
        write_cache_to(
            &cp,
            &UpdateCache {
                checked_at: now_unix_secs(),
                latest_version: "1.2.0".to_string(),
                release_url: "https://github.com/hatimmakki/efficient-ai/releases/tag/v1.2.0".to_string(),
            },
        )
        .unwrap();

        let info = cached_available_update_in(&tmp, "1.1.0");
        assert!(info.is_some());
        assert_eq!(info.unwrap().latest, "1.2.0");
        assert!(cached_available_update_in(&tmp, "1.2.0").is_none());
        let _ = fs::remove_dir_all(&tmp);
    }
}
