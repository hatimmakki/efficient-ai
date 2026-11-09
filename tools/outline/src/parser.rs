//! Multi-language structural symbol extractor for Python, Rust, TS/JS, Go, C/Java, and Markdown.

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolSpan {
    pub kind: &'static str,
    pub name: String,
    pub signature: String,
    pub indent: usize,
    pub start_line: usize, // 1-based inclusive
    pub end_line: usize,   // 1-based inclusive
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Python,
    Rust,
    TsJs,
    Go,
    CStyle,
    Markdown,
    Unknown,
}

impl Lang {
    pub fn from_path(path: &Path) -> Self {
        match path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "py" | "pyi" => Lang::Python,
            "rs" => Lang::Rust,
            "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => Lang::TsJs,
            "go" => Lang::Go,
            "c" | "cc" | "cpp" | "h" | "hpp" | "java" | "cs" | "swift" | "kt" => Lang::CStyle,
            "md" | "markdown" => Lang::Markdown,
            _ => Lang::Unknown,
        }
    }
}

pub fn extract_symbols(source: &str, lang: Lang) -> Vec<SymbolSpan> {
    let lines: Vec<&str> = source.lines().collect();
    match lang {
        Lang::Python => extract_python(&lines),
        Lang::Markdown => extract_markdown(&lines),
        Lang::Rust | Lang::TsJs | Lang::Go | Lang::CStyle => extract_brace_lang(&lines, lang),
        Lang::Unknown => Vec::new(),
    }
}

fn leading_spaces(line: &str) -> usize {
    line.chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum()
}

fn extract_python(lines: &[&str]) -> Vec<SymbolSpan> {
    let mut raw: Vec<SymbolSpan> = Vec::new();
    let mut in_triple: Option<&str> = None;
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Track multi-line triple-quoted strings so docstrings aren't misread as symbols
        if let Some(delim) = in_triple {
            if trimmed.contains(delim) {
                in_triple = None;
            }
            i += 1;
            continue;
        }
        for delim in ["\"\"\"", "'''"] {
            let count = trimmed.matches(delim).count();
            if count % 2 == 1 {
                in_triple = Some(delim);
                break;
            }
        }

        let indent = leading_spaces(line);
        if let Some((kind, name)) = match_python_decl(trimmed) {
            let start_line = i + 1;
            // Collect multi-line signature until ':' at end of line (outside parens)
            let mut sig_parts = vec![trimmed.to_string()];
            let mut paren_depth: i32 = trimmed
                .chars()
                .map(|c| match c {
                    '(' | '[' | '{' => 1,
                    ')' | ']' | '}' => -1,
                    _ => 0,
                })
                .sum();
            let mut j = i;
            while (paren_depth > 0 || !lines[j].trim_end().ends_with(':'))
                && j + 1 < lines.len()
                && j - i < 12
            {
                j += 1;
                let next_t = lines[j].trim();
                paren_depth += next_t
                    .chars()
                    .map(|c| match c {
                        '(' | '[' | '{' => 1,
                        ')' | ']' | '}' => -1,
                        _ => 0,
                    })
                    .sum::<i32>();
                sig_parts.push(next_t.to_string());
            }
            let signature = sig_parts.join(" ");
            raw.push(SymbolSpan {
                kind,
                name,
                signature,
                indent,
                start_line,
                end_line: lines.len(),
            });
            i = j + 1;
            continue;
        }
        i += 1;
    }

    // Compute end_line for each Python symbol by finding the next non-empty, non-comment
    // line at `<= indent` (after the symbol's signature).
    for idx in 0..raw.len() {
        let sym_indent = raw[idx].indent;
        let start = raw[idx].start_line;
        let mut last_non_blank = start;
        for (line_idx, line) in lines.iter().enumerate().skip(start) {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            let ind = leading_spaces(line);
            if ind <= sym_indent && !t.starts_with('@') && !t.starts_with(')') && !t.starts_with(']') {
                break;
            }
            last_non_blank = line_idx + 1;
        }
        raw[idx].end_line = last_non_blank.max(start);
    }

    raw
}

fn match_python_decl(trimmed: &str) -> Option<(&'static str, String)> {
    let rest = if let Some(r) = trimmed.strip_prefix("async def ") {
        ("async def", r)
    } else if let Some(r) = trimmed.strip_prefix("def ") {
        ("def", r)
    } else if let Some(r) = trimmed.strip_prefix("class ") {
        ("class", r)
    } else {
        return None;
    };
    let name: String = rest
        .1
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        None
    } else {
        Some((rest.0, name))
    }
}

fn extract_brace_lang(lines: &[&str], lang: Lang) -> Vec<SymbolSpan> {
    let mut symbols: Vec<SymbolSpan> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
            i += 1;
            continue;
        }
        let indent = leading_spaces(line);
        if let Some((kind, name)) = match_brace_decl(trimmed, lang) {
            let start_line = i + 1;
            // Collect signature up to opening '{' or ';'
            let mut sig = trimmed.to_string();
            let mut j = i;
            while !lines[j].contains('{') && !lines[j].trim_end().ends_with(';') && j + 1 < lines.len() && j - i < 8 {
                j += 1;
                sig.push(' ');
                sig.push_str(lines[j].trim());
            }
            if let Some(brace_pos) = sig.find('{') {
                sig = sig[..brace_pos].trim_end().to_string();
            }

            // Find matching closing '}'
            let mut brace_depth: i32 = 0;
            let mut seen_open = false;
            let mut end_line = j + 1;
            for (k, l) in lines.iter().enumerate().skip(i) {
                let lt = l.trim();
                if lt.starts_with("//") {
                    continue;
                }
                for ch in lt.chars() {
                    if ch == '{' {
                        brace_depth += 1;
                        seen_open = true;
                    } else if ch == '}' {
                        brace_depth -= 1;
                    }
                }
                if seen_open && brace_depth <= 0 {
                    end_line = k + 1;
                    break;
                }
                if !seen_open && k >= j && lt.ends_with(';') {
                    end_line = k + 1;
                    break;
                }
            }

            symbols.push(SymbolSpan {
                kind,
                name,
                signature: sig,
                indent,
                start_line,
                end_line,
            });
        }
        i += 1;
    }

    symbols
}

fn match_brace_decl(trimmed: &str, lang: Lang) -> Option<(&'static str, String)> {
    // Strip common visibility / modifiers
    let mut s = trimmed;
    for prefix in [
        "pub(crate) ",
        "pub(super) ",
        "pub ",
        "export default ",
        "export ",
        "async ",
        "unsafe ",
        "const ",
        "static ",
        "public ",
        "private ",
        "protected ",
        "internal ",
        "virtual ",
        "override ",
    ] {
        while let Some(rest) = s.strip_prefix(prefix) {
            s = rest.trim_start();
        }
    }

    let keywords: &[(&str, &str)] = match lang {
        Lang::Rust => &[
            ("fn ", "fn"),
            ("struct ", "struct"),
            ("enum ", "enum"),
            ("trait ", "trait"),
            ("impl", "impl"),
            ("type ", "type"),
            ("mod ", "mod"),
        ],
        Lang::TsJs => &[
            ("function ", "function"),
            ("class ", "class"),
            ("interface ", "interface"),
            ("type ", "type"),
            ("enum ", "enum"),
        ],
        Lang::Go => &[
            ("func ", "func"),
            ("type ", "type"),
        ],
        Lang::CStyle => &[
            ("class ", "class"),
            ("struct ", "struct"),
            ("interface ", "interface"),
            ("enum ", "enum"),
        ],
        _ => &[],
    };

    for (kw, kind) in keywords {
        if let Some(rest) = s.strip_prefix(kw) {
            let rest = rest.trim_start();
            // Handle Go receiver `func (r *Receiver) Name(...)` or Rust `impl<T> Type`
            let after_generics_or_recv = if rest.starts_with('(') {
                rest.find(')').map(|p| rest[p + 1..].trim_start()).unwrap_or(rest)
            } else if rest.starts_with('<') {
                rest.find('>').map(|p| rest[p + 1..].trim_start()).unwrap_or(rest)
            } else {
                rest
            };
            let name: String = after_generics_or_recv
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
                .collect();
            if !name.is_empty() {
                return Some((kind, name));
            }
        }
    }
    None
}

fn extract_markdown(lines: &[&str]) -> Vec<SymbolSpan> {
    let mut symbols: Vec<SymbolSpan> = Vec::new();
    let mut in_fence = false;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if t.starts_with('#') {
            let hashes = t.chars().take_while(|c| *c == '#').count();
            if hashes >= 1 && hashes <= 6 && t.chars().nth(hashes) == Some(' ') {
                let title = t[hashes..].trim().to_string();
                symbols.push(SymbolSpan {
                    kind: "heading",
                    name: title.clone(),
                    signature: t.to_string(),
                    indent: (hashes - 1) * 2,
                    start_line: i + 1,
                    end_line: lines.len(),
                });
            }
        }
    }
    for idx in 0..symbols.len() {
        if idx + 1 < symbols.len() {
            symbols[idx].end_line = symbols[idx + 1].start_line.saturating_sub(1);
        }
    }
    symbols
}

/// Find the innermost symbol span containing 1-based `line_no`.
pub fn enclosing_symbol(symbols: &[SymbolSpan], line_no: usize) -> Option<&SymbolSpan> {
    symbols
        .iter()
        .filter(|s| line_no >= s.start_line && line_no <= s.end_line)
        .min_by_key(|s| s.end_line.saturating_sub(s.start_line))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_python_symbols_and_enclosing() {
        let src = r#"
class TokenAnalyzer:
    """Docstring that should not break parsing."""

    def __init__(self, root: str):
        self.root = root

    async def compute_metrics(
        self,
        limit: int = 10,
    ) -> int:
        # inside compute_metrics
        return limit * 2

def helper_fn(x: int) -> int:
    return x + 1
"#;
        let syms = extract_symbols(src, Lang::Python);
        assert_eq!(syms.len(), 4);
        assert_eq!(syms[0].name, "TokenAnalyzer");
        assert_eq!(syms[1].name, "__init__");
        assert_eq!(syms[2].name, "compute_metrics");
        assert_eq!(syms[3].name, "helper_fn");

        let enc = enclosing_symbol(&syms, 12).expect("should find enclosing symbol");
        assert_eq!(enc.name, "compute_metrics");
    }

    #[test]
    fn test_rust_symbols() {
        let src = r#"
pub struct Config {
    pub limit: usize,
}

impl Config {
    pub fn new(limit: usize) -> Self {
        Self { limit }
    }
}
"#;
        let syms = extract_symbols(src, Lang::Rust);
        assert_eq!(syms.len(), 3);
        assert_eq!(syms[0].name, "Config");
        assert_eq!(syms[2].name, "new");
    }
}
