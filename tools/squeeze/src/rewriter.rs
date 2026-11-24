//! H8 Machine-friendly CLI flag rewriter (`squeeze --rewrite "<cmd>"`).

/// Check whether a single shell command is a noisy test/build runner that benefits from `squeeze --`.
pub fn should_wrap_command(cmd: &str) -> bool {
    let trimmed = cmd.trim();
    if trimmed.is_empty()
        || trimmed.contains('|')
        || trimmed.contains("&&")
        || trimmed.contains(';')
        || trimmed.contains('\n')
        || trimmed.starts_with("squeeze ")
        || trimmed.starts_with("eai ")
    {
        return false;
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.is_empty() {
        return false;
    }

    match parts[0] {
        "pytest" | "vitest" | "jest" | "mocha" | "tsc" | "eslint" | "mypy" | "ruff" => true,
        "npx" => matches!(parts.get(1).copied(), Some("vitest" | "jest" | "tsc" | "eslint")),
        "cargo" => matches!(parts.get(1).copied(), Some("test" | "build" | "check" | "clippy")),
        "npm" | "pnpm" | "yarn" | "bun" => {
            matches!(parts.get(1).copied(), Some("test"))
                || (parts.get(1).copied() == Some("run")
                    && matches!(parts.get(2).copied(), Some("test" | "build" | "lint" | "typecheck")))
        }
        "go" => matches!(parts.get(1).copied(), Some("test" | "build" | "vet")),
        "dotnet" => matches!(parts.get(1).copied(), Some("test" | "build")),
        "flutter" => matches!(parts.get(1).copied(), Some("test" | "analyze")),
        "mvn" | "gradle" | "./gradlew" | "gradlew" => {
            matches!(parts.get(1).copied(), Some("test" | "build" | "check" | "verify"))
        }
        "docker" => matches!(parts.get(1).copied(), Some("build")),
        "python" | "python3" | "py" => {
            parts.get(1).copied() == Some("-m")
                && matches!(parts.get(2).copied(), Some("pytest" | "unittest" | "mypy"))
        }
        "uv" => {
            parts.get(1).copied() == Some("run")
                && matches!(parts.get(2).copied(), Some("pytest" | "mypy" | "ruff"))
        }
        _ => false,
    }
}

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
                if !parts
                    .iter()
                    .any(|p| *p == "-s" || *p == "-sb" || *p == "--short" || *p == "--porcelain")
                {
                    return format!("{} -sb", trimmed);
                }
            }
            "log" => {
                let mut out = trimmed.to_string();
                if !parts.iter().any(|p| {
                    p.starts_with("--oneline")
                        || p.starts_with("--format")
                        || p.starts_with("--pretty")
                }) {
                    out.push_str(" --oneline");
                }
                if !parts.iter().any(|p| {
                    *p == "-n"
                        || (p.starts_with('-')
                            && p.len() > 1
                            && p[1..].chars().all(|c| c.is_ascii_digit()))
                }) {
                    out.push_str(" -n 15");
                }
                return out;
            }
            "diff" if !parts.contains(&"--no-pager") => {
                return trimmed.replacen("git diff", "git --no-pager diff", 1);
            }
            _ => {}
        },
        "pytest"
            if !parts
                .iter()
                .any(|p| *p == "-q" || *p == "--quiet" || *p == "-v" || *p == "-vv") =>
        {
            return format!("{} -q --tb=short", trimmed);
        }
        "cargo"
            if matches!(parts.get(1).copied(), Some("test" | "build" | "check"))
                && !parts
                    .iter()
                    .any(|p| *p == "-q" || *p == "--quiet" || *p == "--") =>
        {
            return format!("{} -q", trimmed);
        }
        _ => {}
    }

    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_and_runner_rewrites() {
        assert_eq!(rewrite_command("git status"), "git status -sb");
        assert_eq!(rewrite_command("git status -sb"), "git status -sb");
        assert_eq!(rewrite_command("git log"), "git log --oneline -n 15");
        assert_eq!(rewrite_command("git diff src/"), "git --no-pager diff src/");
        assert_eq!(rewrite_command("pytest tests/"), "pytest tests/ -q --tb=short");
        assert_eq!(rewrite_command("cargo test"), "cargo test -q");
        assert_eq!(rewrite_command("cargo check"), "cargo check -q");
    }

    #[test]
    fn test_should_wrap_command() {
        assert!(should_wrap_command("pytest tests/"));
        assert!(should_wrap_command("python -m pytest"));
        assert!(should_wrap_command("uv run pytest -v"));
        assert!(should_wrap_command("npm run test"));
        assert!(should_wrap_command("pnpm test"));
        assert!(should_wrap_command("go test ./..."));
        assert!(should_wrap_command("cargo clippy"));
        assert!(!should_wrap_command("git status"));
        assert!(!should_wrap_command("squeeze -- pytest"));
        assert!(!should_wrap_command("pytest | head"));
    }
}
