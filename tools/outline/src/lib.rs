//! `outline`: Structure-aware code outlines, targeted symbol reads, and enclosing-symbol search.

pub mod parser;

use parser::{enclosing_symbol, extract_symbols, Lang};
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
        "outline 0.2.0 — Structure-aware file reader & enclosing-symbol search\n\
         \n\
         USAGE:\n\
         \x20 outline <file_or_dir>                      Show symbol outline with [Lstart-Lend]\n\
         \x20 outline <file> --symbol <name>             Read full definition of symbol <name>\n\
         \x20 outline <file> --lines <start>-<end>       Read specific 1-based line range\n\
         \x20 outline search <pattern> [path] [--expand] Search and resolve matches to enclosing symbol\n"
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

    let target = PathBuf::from(&args[0]);
    let mut symbol_filter: Option<String> = None;
    let mut line_range: Option<(usize, usize)> = None;

    let mut i = 1;
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
            other => {
                eprintln!("Error: unknown flag '{}'", other);
                return (ExitCode::from(2), None);
            }
        }
    }

    if !target.exists() {
        eprintln!("Error: path not found: {}", target.display());
        return (ExitCode::from(1), None);
    }

    if target.is_dir() {
        let mut files = Vec::new();
        collect_source_files(&target, &mut files);
        let mut total_raw = 0usize;
        let mut total_out = 0usize;
        let mut total_lines = 0usize;
        let mut total_syms = 0usize;
        for f in files {
            if let Ok((rendered, m)) = format_file_outline(&f) {
                println!("{}", rendered);
                total_raw += m.raw_chars;
                total_out += m.output_chars;
                total_lines += m.total_lines;
                total_syms += m.symbols;
            }
        }
        return (
            ExitCode::SUCCESS,
            Some(OutlineMetrics {
                action: "outline_dir",
                target: target.display().to_string(),
                raw_chars: total_raw,
                output_chars: total_out,
                total_lines,
                symbols: total_syms,
            }),
        );
    }

    if let Some((start, end)) = line_range {
        match format_line_range(&target, start, end) {
            Ok((rendered, m)) => {
                print!("{}", rendered);
                (ExitCode::SUCCESS, Some(m))
            }
            Err(e) => {
                eprintln!("Error reading {}: {}", target.display(), e);
                (ExitCode::from(1), None)
            }
        }
    } else if let Some(sym_query) = symbol_filter {
        match format_symbol(&target, &sym_query) {
            Ok((rendered, m)) => {
                print!("{}", rendered);
                (ExitCode::SUCCESS, Some(m))
            }
            Err(e) => {
                eprintln!("Error reading {}: {}", target.display(), e);
                (ExitCode::from(1), None)
            }
        }
    } else {
        match format_file_outline(&target) {
            Ok((rendered, m)) => {
                print!("{}", rendered);
                (ExitCode::SUCCESS, Some(m))
            }
            Err(e) => {
                eprintln!("Error reading {}: {}", target.display(), e);
                (ExitCode::from(1), None)
            }
        }
    }
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
    let src = fs::read_to_string(path)?;
    let lines: Vec<&str> = src.lines().collect();
    let total_lines = lines.len();
    let lang = Lang::from_path(path);
    let syms = extract_symbols(&src, lang);

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
        for sym in &syms {
            let pad = " ".repeat(sym.indent);
            out.push_str(&format!(
                "{}[L{}-L{}] {}\n",
                pad, sym.start_line, sym.end_line, sym.signature
            ));
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
    let lang = Lang::from_path(path);
    let syms = extract_symbols(&src, lang);
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

    let mut files = Vec::new();
    if root.is_file() {
        files.push(root.clone());
    } else {
        collect_source_files(&root, &mut files);
    }

    let pat_norm = if case_insensitive {
        pattern.to_ascii_lowercase()
    } else {
        pattern.clone()
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

        let lang = Lang::from_path(&file);
        let syms = extract_symbols(&src, lang);

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
                    out.push_str(&format!("    {:>4}: {}\n", line_no, lines[line_no - 1].trim()));
                }
            } else {
                out.push_str(&format!("    {:>4}: {}\n", line_no, lines[line_no - 1].trim()));
            }
        }
    }

    out.push_str(&format!(
        "[{} matches across {} files; use 'outline <file> --symbol <name>' or '--expand' to read full symbol]\n",
        total_matches, matched_files
    ));
    print!("{}", out);

    (
        ExitCode::SUCCESS,
        Some(OutlineMetrics {
            action: "outline_search",
            target: root.display().to_string(),
            raw_chars,
            output_chars: out.len(),
            total_lines: total_matches,
            symbols: matched_files,
        }),
    )
}

pub fn collect_source_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut sorted: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    sorted.sort_by_key(|e| e.file_name());

    for entry in sorted {
        let p = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if p.is_dir() {
            if !SKIP_DIRS.contains(&name_str.as_ref()) && !name_str.starts_with('.') {
                collect_source_files(&p, out);
            }
        } else if p.is_file() && Lang::from_path(&p) != Lang::Unknown {
            out.push(p);
        }
    }
}
