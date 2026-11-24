//! `outline`: Structure-aware code outlines (tree-sitter + .gitignore + token budget),
//! targeted symbol reads, and enclosing-symbol search.

pub mod parser;
#[cfg(feature = "treesitter")]
pub mod ts_parser;

use ignore::WalkBuilder;
use parser::{enclosing_symbol, extract_symbols_for_path, Lang};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub const SKIP_DIRS: &[&str] = &[
    ".git",
    ".venv",
    "venv",
    "node_modules",
    "target",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    "dist",
    "build",
    "out",
    "coverage",
    "vendor",
    ".next",
    ".nuxt",
    ".svelte-kit",
    ".turbo",
    ".gradle",
    "Pods",
    "bin",
    "obj",
    ".eai",
];

#[derive(Debug, Clone, Default)]
pub struct OutlineMetrics {
    pub action: &'static str,
    pub target: String,
    pub raw_chars: usize,
    pub output_chars: usize,
    pub total_lines: usize,
    pub symbols: usize,
}

pub fn print_usage() {
    eprintln!(
        "outline 0.3.0 — Structure-aware file reader (tree-sitter + .gitignore + token budget)\n\
         \n\
         USAGE:\n\
         \x20 outline <file_or_dir>... [--budget N]        Show symbol outline with [Lstart-Lend]\n\
         \x20 outline <file> --symbol <name>               Read full definition of symbol <name>\n\
         \x20 outline <file> --lines <start>-<end>         Read specific 1-based line range\n\
         \x20 outline search <pattern> [path] [--expand]   Search and resolve matches to enclosing symbol\n"
    );
}

pub fn run_cli(args: &[String]) -> (ExitCode, Option<OutlineMetrics>) {
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        print_usage();
        return (ExitCode::SUCCESS, None);
    }

    if args[0] == "search" {
        return cmd_search(&args[1..]);
    }

    let mut targets: Vec<PathBuf> = Vec::new();
    let mut symbol_filter: Option<String> = None;
    let mut line_range: Option<(usize, usize)> = None;
    let mut token_budget: Option<usize> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--symbol" | "-s" if i + 1 < args.len() => {
                symbol_filter = Some(args[i + 1].clone());
                i += 2;
            }
            "--lines" | "-l" if i + 1 < args.len() => {
                if let Some(r) = parse_range(&args[i + 1]) {
                    line_range = Some(r);
                } else {
                    eprintln!(
                        "Error: invalid line range '{}' (expected START-END)",
                        args[i + 1]
                    );
                    return (ExitCode::from(2), None);
                }
                i += 2;
            }
            "--budget" | "-b" if i + 1 < args.len() => {
                token_budget = args[i + 1].parse().ok();
                i += 2;
            }
            other if other.starts_with('-') => {
                eprintln!("Error: unknown flag '{}'", other);
                return (ExitCode::from(2), None);
            }
            path_arg => {
                targets.push(PathBuf::from(path_arg));
                i += 1;
            }
        }
    }

    if targets.is_empty() {
        print_usage();
        return (ExitCode::from(2), None);
    }

    // Single file with --lines or --symbol
    if targets.len() == 1 && targets[0].is_file() {
        let target = &targets[0];
        if let Some((start, end)) = line_range {
            return match format_line_range(target, start, end) {
                Ok((rendered, m)) => {
                    print!("{}", rendered);
                    (ExitCode::SUCCESS, Some(m))
                }
                Err(e) => {
                    eprintln!("Error reading {}: {}", target.display(), e);
                    (ExitCode::from(1), None)
                }
            };
        }
        if let Some(sym_query) = symbol_filter {
            return match format_symbol(target, &sym_query) {
                Ok((rendered, m)) => {
                    print!("{}", rendered);
                    (ExitCode::SUCCESS, Some(m))
                }
                Err(e) => {
                    eprintln!("Error reading {}: {}", target.display(), e);
                    (ExitCode::from(1), None)
                }
            };
        }
    }

    // One or more files/directories -> batched outline (supports --budget)
    let mut files = Vec::new();
    for t in &targets {
        if !t.exists() {
            eprintln!("Error: path not found: {}", t.display());
            return (ExitCode::from(1), None);
        }
        if t.is_dir() {
            collect_source_files(t, &mut files);
        } else {
            files.push(t.clone());
        }
    }

    let mut total_raw = 0usize;
    let mut total_out = 0usize;
    let mut total_lines = 0usize;
    let mut total_syms = 0usize;
    let mut rendered_all = String::new();
    let max_chars = token_budget.map(|b| b.saturating_mul(4));
    let mut truncated_files = 0usize;

    for (idx, f) in files.iter().enumerate() {
        if let Some(limit) = max_chars {
            if rendered_all.len() >= limit {
                truncated_files = files.len() - idx;
                break;
            }
        }
        let per_file_budget = max_chars.map(|lim| lim.saturating_sub(rendered_all.len()));
        if let Ok((rendered, m)) = format_file_outline_budgeted(f, per_file_budget) {
            if !rendered_all.is_empty() {
                rendered_all.push('\n');
            }
            rendered_all.push_str(&rendered);
            total_raw += m.raw_chars;
            total_out += rendered.len();
            total_lines += m.total_lines;
            total_syms += m.symbols;
        }
    }

    if truncated_files > 0 {
        rendered_all.push_str(&format!(
            "\n[... {} more files omitted to fit --budget {} tokens]\n",
            truncated_files,
            token_budget.unwrap_or(0)
        ));
    }

    print!("{}", rendered_all);

    let action = if files.len() > 1 {
        "outline_batch"
    } else {
        "outline"
    };
    (
        ExitCode::SUCCESS,
        Some(OutlineMetrics {
            action,
            target: targets[0].display().to_string(),
            raw_chars: total_raw,
            output_chars: total_out,
            total_lines,
            symbols: total_syms,
        }),
    )
}

pub fn parse_range(spec: &str) -> Option<(usize, usize)> {
    let spec = spec.trim().trim_start_matches('L').trim_start_matches('l');
    if let Some((a, b)) = spec.split_once('-').or_else(|| spec.split_once(':')) {
        let s: usize = a.trim().trim_start_matches('L').parse().ok()?;
        let e: usize = b.trim().trim_start_matches('L').parse().ok()?;
        Some((s.max(1), e.max(s.max(1))))
    } else {
        let s: usize = spec.parse().ok()?;
        Some((s.max(1), s.max(1)))
    }
}

pub fn format_file_outline(path: &Path) -> io::Result<(String, OutlineMetrics)> {
    format_file_outline_budgeted(path, None)
}

pub fn format_file_outline_budgeted(
    path: &Path,
    max_chars: Option<usize>,
) -> io::Result<(String, OutlineMetrics)> {
    let src = fs::read_to_string(path)?;
    let lines: Vec<&str> = src.lines().collect();
    let total_lines = lines.len();
    let syms = extract_symbols_for_path(&src, path);

    let mut out = String::new();
    out.push_str(&format!("# {} ({} lines)\n", path.display(), total_lines));

    if syms.is_empty() {
        let preview = total_lines.min(25);
        for (i, line) in lines.iter().take(preview).enumerate() {
            out.push_str(&format!("{:>4}: {}\n", i + 1, line));
        }
        if total_lines > preview {
            out.push_str(&format!(
                "[... {} more lines; use 'outline {} --lines 26-{}']\n",
                total_lines - preview,
                path.display(),
                total_lines
            ));
        }
    } else {
        for (emitted, sym) in syms.iter().enumerate() {
            let pad = " ".repeat(sym.indent);
            let line = format!(
                "{}[L{}-L{}] {}\n",
                pad, sym.start_line, sym.end_line, sym.signature
            );
            if let Some(cap) = max_chars {
                if emitted > 0 && out.len() + line.len() > cap.saturating_sub(120) {
                    out.push_str(&format!(
                        "[... {} more symbols omitted to fit token budget]\n",
                        syms.len() - emitted
                    ));
                    break;
                }
            }
            out.push_str(&line);
        }
        out.push_str(&format!(
            "[{} symbols across {} lines; read symbol: 'outline {} --symbol <name>' or '--lines <start>-<end>']\n",
            syms.len(),
            total_lines,
            path.display()
        ));
    }

    let metrics = OutlineMetrics {
        action: "outline",
        target: path.display().to_string(),
        raw_chars: src.len(),
        output_chars: out.len(),
        total_lines,
        symbols: syms.len(),
    };
    Ok((out, metrics))
}

pub fn format_symbol(path: &Path, query: &str) -> io::Result<(String, OutlineMetrics)> {
    let src = fs::read_to_string(path)?;
    let lines: Vec<&str> = src.lines().collect();
    let syms = extract_symbols_for_path(&src, path);
    let sym_part = query.rsplit('.').next().unwrap_or(query);
    let q_lower = sym_part.to_ascii_lowercase();

    let exact: Vec<_> = syms
        .iter()
        .filter(|s| s.name.to_ascii_lowercase() == q_lower)
        .collect();

    let raw_matches: Vec<_> = if !exact.is_empty() {
        exact
    } else {
        syms.iter()
            .filter(|s| {
                let n = s.name.to_ascii_lowercase();
                n.contains(&q_lower) || s.signature.to_ascii_lowercase().contains(&q_lower)
            })
            .collect()
    };

    if raw_matches.is_empty() {
        eprintln!(
            "No symbol matching '{}' in {}; showing outline:",
            query,
            path.display()
        );
        return format_file_outline(path);
    }

    let mut out = String::new();
    let mut printed_spans: Vec<(usize, usize)> = Vec::new();
    for sym in raw_matches {
        if printed_spans
            .iter()
            .any(|(s, e)| sym.start_line >= *s && sym.end_line <= *e)
        {
            continue;
        }
        printed_spans.push((sym.start_line, sym.end_line));
        out.push_str(&format!(
            "# {} :: {} [L{}-L{}]\n",
            path.display(),
            sym.name,
            sym.start_line,
            sym.end_line
        ));
        let start_idx = sym.start_line.saturating_sub(1);
        let end_idx = sym.end_line.min(lines.len());
        for (idx, line) in lines.iter().enumerate().take(end_idx).skip(start_idx) {
            out.push_str(&format!("{:>4}: {}\n", idx + 1, line));
        }
    }

    let metrics = OutlineMetrics {
        action: "outline_symbol",
        target: path.display().to_string(),
        raw_chars: src.len(),
        output_chars: out.len(),
        total_lines: lines.len(),
        symbols: printed_spans.len(),
    };
    Ok((out, metrics))
}

pub fn format_line_range(
    path: &Path,
    start: usize,
    end: usize,
) -> io::Result<(String, OutlineMetrics)> {
    let src = fs::read_to_string(path)?;
    let lines: Vec<&str> = src.lines().collect();
    let total = lines.len();
    let s_idx = start.saturating_sub(1).min(total);
    let e_idx = end.min(total);

    let mut out = String::new();
    out.push_str(&format!(
        "# {} [L{}-L{} of {}]\n",
        path.display(),
        s_idx + 1,
        e_idx,
        total
    ));
    for (idx, line) in lines.iter().enumerate().take(e_idx).skip(s_idx) {
        out.push_str(&format!("{:>4}: {}\n", idx + 1, line));
    }

    let metrics = OutlineMetrics {
        action: "outline_lines",
        target: path.display().to_string(),
        raw_chars: src.len(),
        output_chars: out.len(),
        total_lines: total,
        symbols: 0,
    };
    Ok((out, metrics))
}

pub fn format_search(
    pattern: &str,
    root: &Path,
    expand: bool,
    case_insensitive: bool,
) -> (String, OutlineMetrics) {
    let mut files = Vec::new();
    if root.is_file() {
        files.push(root.to_path_buf());
    } else {
        collect_source_files(root, &mut files);
    }

    let pat_norm = if case_insensitive {
        pattern.to_ascii_lowercase()
    } else {
        pattern.to_string()
    };

    let mut total_matches = 0usize;
    let mut matched_files = 0usize;
    let mut raw_chars = 0usize;
    let mut out = String::new();

    for file in files {
        let Ok(src) = fs::read_to_string(&file) else {
            continue;
        };
        let lines: Vec<&str> = src.lines().collect();
        let mut hit_lines = Vec::new();
        for (idx, line) in lines.iter().enumerate() {
            let matched = if case_insensitive {
                line.to_ascii_lowercase().contains(&pat_norm)
            } else {
                line.contains(&pat_norm)
            };
            if matched {
                hit_lines.push(idx + 1);
            }
        }
        if hit_lines.is_empty() {
            continue;
        }
        matched_files += 1;
        total_matches += hit_lines.len();
        raw_chars += src.len();

        let syms = extract_symbols_for_path(&src, &file);

        out.push_str(&format!("# {}\n", file.display()));
        let mut last_printed_sym: Option<(usize, usize)> = None;

        for line_no in hit_lines {
            if let Some(sym) = enclosing_symbol(&syms, line_no) {
                let span_key = (sym.start_line, sym.end_line);
                if last_printed_sym != Some(span_key) {
                    out.push_str(&format!(
                        "  [L{}-L{}] {}\n",
                        sym.start_line, sym.end_line, sym.signature
                    ));
                    if expand {
                        for (idx, line) in lines
                            .iter()
                            .enumerate()
                            .take(sym.end_line.min(lines.len()))
                            .skip(sym.start_line.saturating_sub(1))
                        {
                            out.push_str(&format!("    {:>4}: {}\n", idx + 1, line));
                        }
                    }
                    last_printed_sym = Some(span_key);
                }
                if !expand {
                    out.push_str(&format!(
                        "    {:>4}: {}\n",
                        line_no,
                        lines[line_no - 1].trim()
                    ));
                }
            } else {
                out.push_str(&format!(
                    "    {:>4}: {}\n",
                    line_no,
                    lines[line_no - 1].trim()
                ));
            }
        }
    }

    out.push_str(&format!(
        "[{} matches across {} files; use 'outline <file> --symbol <name>' or '--expand' to read full symbol]\n",
        total_matches, matched_files
    ));

    let metrics = OutlineMetrics {
        action: "outline_search",
        target: root.display().to_string(),
        raw_chars,
        output_chars: out.len(),
        total_lines: total_matches,
        symbols: matched_files,
    };
    (out, metrics)
}

fn cmd_search(args: &[String]) -> (ExitCode, Option<OutlineMetrics>) {
    if args.is_empty() {
        eprintln!("Error: 'outline search' requires a <pattern>");
        return (ExitCode::from(2), None);
    }
    let pattern = &args[0];
    let mut root = PathBuf::from(".");
    let mut expand = false;
    let mut case_insensitive = false;

    for arg in &args[1..] {
        match arg.as_str() {
            "--expand" | "-e" => expand = true,
            "-i" | "--ignore-case" => case_insensitive = true,
            other => root = PathBuf::from(other),
        }
    }

    let (rendered, metrics) = format_search(pattern, &root, expand, case_insensitive);
    print!("{}", rendered);
    (ExitCode::SUCCESS, Some(metrics))
}

/// `.gitignore`-aware recursive source file collector using the `ignore` crate.
pub fn collect_source_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut walker = WalkBuilder::new(dir);
    walker
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .sort_by_file_name(|a, b| a.cmp(b));

    for entry in walker.build().flatten() {
        let p = entry.path();
        if p.is_dir() {
            continue;
        }
        if p.components().any(|c| {
            let s = c.as_os_str().to_string_lossy();
            SKIP_DIRS.contains(&s.as_ref())
        }) {
            continue;
        }
        if p.is_file() && Lang::from_path(p) != Lang::Unknown {
            out.push(p.to_path_buf());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "treesitter")]
    fn test_treesitter_tsx_arrow_and_class() {
        let src = r#"
export const UserCard = ({ name }: { name: string }) => {
  const label = "{ braced string }";
  return <div>{name}{label}</div>;
};

export class Store {
  async load(id: string) {
    return id;
  }
}
"#;
        let syms = extract_symbols_for_path(src, Path::new("UserCard.tsx"));
        let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["UserCard", "Store", "load"]);
        assert_eq!(syms[0].start_line, 2);
        assert_eq!(syms[0].end_line, 5);
    }

    #[test]
    fn test_json_and_toml_outlines() {
        let json_src = "{\n  \"name\": \"eai\",\n  \"scripts\": {\n    \"test\": \"cargo test\"\n  }\n}\n";
        let syms = extract_symbols_for_path(json_src, Path::new("package.json"));
        let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"name"));
        assert!(names.contains(&"scripts"));

        let toml_src = "[package]\nname = \"eai\"\n\n[dependencies]\nserde = \"1\"\n";
        let tsyms = extract_symbols_for_path(toml_src, Path::new("Cargo.toml"));
        let tnames: Vec<&str> = tsyms.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(tnames, vec!["package", "dependencies"]);
    }
}
