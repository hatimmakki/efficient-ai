//! H8 Machine-friendly CLI flag rewriter (`squeeze --rewrite "<cmd>"`).

/// Rewrite a shell command string to add non-interactive, token-efficient flags where safe.
pub fn rewrite_command(cmd: &str) -> String {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // Don't rewrite pipelines or compound commands that already include custom formatting
    if trimmed.contains('|') || trimmed.contains("&&") || trimmed.contains(';') {
        return trimmed.to_string();
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.is_empty() {
        return trimmed.to_string();
    }

    match parts[0] {
        "git" if parts.len() >= 2 => match parts[1] {
            "status" => {
                if !parts.iter().any(|p| *p == "-s" || *p == "-sb" || *p == "--short" || *p == "--porcelain") {
                    return format!("{} -sb", trimmed);
                }
            }
            "log" => {
                let mut out = trimmed.to_string();
                if !parts.iter().any(|p| p.starts_with("--oneline") || p.starts_with("--format") || p.starts_with("--pretty")) {
                    out.push_str(" --oneline");
                }
                if !parts.iter().any(|p| *p == "-n" || p.starts_with('-') && p[1..].chars().all(|c| c.is_ascii_digit())) {
                    out.push_str(" -n 15");
                }
                return out;
            }
            "diff" => {
                if !parts.iter().any(|p| *p == "--no-pager") {
                    return trimmed.replacen("git diff", "git --no-pager diff", 1);
                }
            }
            _ => {}
        },
        "pytest" => {
            if !parts.iter().any(|p| *p == "-q" || *p == "--quiet" || *p == "-v" || *p == "-vv") {
                return format!("{} -q --tb=short", trimmed);
            }
        }
        "cargo" if parts.get(1) == Some(&"test") => {
            if !parts.iter().any(|p| *p == "-q" || *p == "--quiet" || *p == "--") {
                return format!("{} -q", trimmed);
            }
        }
        _ => {}
    }

    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_rewrites() {
        assert_eq!(rewrite_command("git status"), "git status -sb");
        assert_eq!(rewrite_command("git status -sb"), "git status -sb");
        assert_eq!(rewrite_command("git log"), "git log --oneline -n 15");
        assert_eq!(rewrite_command("git diff src/"), "git --no-pager diff src/");
        assert_eq!(rewrite_command("pytest tests/"), "pytest tests/ -q --tb=short");
        assert_eq!(rewrite_command("cargo test"), "cargo test -q");
    }
}
