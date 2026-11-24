//! Multi-language structural symbol extractor for Python, Rust, TS/JS, Go, C/Java/C#/Swift/Kotlin, and Markdown.

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
            "c" | "cc" | "cpp" | "cxx" | "h" | "hpp" | "java" | "cs" | "swift" | "kt" | "kts"
            | "dart" => Lang::CStyle,
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

fn update_triple_quote(trimmed: &str, in_triple: &mut Option<&str>) {
    if let Some(delim) = *in_triple {
        if trimmed.matches(delim).count() % 2 == 1 {
            *in_triple = None;
        }
        return;
    }
    for delim in ["\"\"\"", "'''"] {
        if trimmed.matches(delim).count() % 2 == 1 {
            *in_triple = Some(delim);
            return;
        }
    }
}

fn extract_python(lines: &[&str]) -> Vec<SymbolSpan> {
    let mut raw: Vec<(SymbolSpan, usize)> = Vec::new(); // (span, sig_end_idx)
    let mut in_triple: Option<&str> = None;
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        if in_triple.is_some() {
            update_triple_quote(trimmed, &mut in_triple);
            i += 1;
            continue;
        }
        if !trimmed.starts_with('#') {
            update_triple_quote(trimmed, &mut in_triple);
        }

        let indent = leading_spaces(line);
        if let Some((kind, name)) = match_python_decl(trimmed) {
            // Walk backward to include contiguous @decorator lines at the same indent
            let mut decl_start_idx = i;
            while decl_start_idx > 0 {
                let prev = lines[decl_start_idx - 1];
                let pt = prev.trim();
                if pt.starts_with('@') && leading_spaces(prev) == indent {
                    decl_start_idx -= 1;
                } else {
                    break;
                }
            }
            let start_line = decl_start_idx + 1;

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
            raw.push((
                SymbolSpan {
                    kind,
                    name,
                    signature,
                    indent,
                    start_line,
                    end_line: lines.len(),
                },
                j,
            ));
            i = j + 1;
            continue;
        }
        i += 1;
    }

    // Compute end_line for each Python symbol while respecting multi-line triple-quoted strings
    for (sym, sig_end_idx) in &mut raw {
        let sym_indent = sym.indent;
        let mut last_non_blank = *sig_end_idx + 1;
        let mut body_triple: Option<&str> = None;

        for (line_idx, line) in lines.iter().enumerate().skip(*sig_end_idx + 1) {
            let t = line.trim();
            if body_triple.is_some() {
                update_triple_quote(t, &mut body_triple);
                if !t.is_empty() {
                    last_non_blank = line_idx + 1;
                }
                continue;
            }
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            let ind = leading_spaces(line);
            if ind <= sym_indent
                && !t.starts_with(')')
                && !t.starts_with(']')
                && !t.starts_with('}')
            {
                break;
            }
            update_triple_quote(t, &mut body_triple);
            last_non_blank = line_idx + 1;
        }
        sym.end_line = last_non_blank.max(sym.start_line);
    }

    raw.into_iter().map(|(s, _)| s).collect()
}

fn match_python_decl(trimmed: &str) -> Option<(&'static str, String)> {
    let rest = if let Some(r) = trimmed.strip_prefix("async def ") {
        ("async def", r)
    } else if let Some(r) = trimmed.strip_prefix("def ") {
        ("def", r)
    } else {
        ("class", trimmed.strip_prefix("class ")?)
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

/// Scan braces on a single line while ignoring strings, char literals, line comments, and block comments.
fn scan_braces_in_line(line: &str, in_block_comment: &mut bool) -> (i32, bool) {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut delta = 0i32;
    let mut saw_open = false;
    let mut in_quote: Option<u8> = None;

    while i < len {
        if *in_block_comment {
            if i + 1 < len && bytes[i] == b'*' && bytes[i + 1] == b'/' {
                *in_block_comment = false;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }

        if let Some(q) = in_quote {
            if bytes[i] == b'\\' {
                i += 2;
                continue;
            }
            if bytes[i] == q {
                in_quote = None;
            }
            i += 1;
            continue;
        }

        if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            break;
        }
        if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            *in_block_comment = true;
            i += 2;
            continue;
        }

        match bytes[i] {
            b'"' | b'`' => {
                in_quote = Some(bytes[i]);
            }
            b'\'' => {
                // Avoid confusing Rust lifetimes `'a` with char literals: only treat as char/JS quote if closed on line
                if line[i + 1..].contains('\'') {
                    in_quote = Some(b'\'');
                }
            }
            b'{' => {
                delta += 1;
                saw_open = true;
            }
            b'}' => {
                delta -= 1;
            }
            _ => {}
        }
        i += 1;
    }

    (delta, saw_open)
}

fn extract_brace_lang(lines: &[&str], lang: Lang) -> Vec<SymbolSpan> {
    let mut symbols: Vec<SymbolSpan> = Vec::new();
    let mut in_block_comment = false;
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        if in_block_comment {
            if trimmed.contains("*/") {
                in_block_comment = false;
            }
            i += 1;
            continue;
        }
        if trimmed.starts_with("/*") && !trimmed.contains("*/") {
            in_block_comment = true;
            i += 1;
            continue;
        }
        if trimmed.is_empty()
            || trimmed.starts_with("//")
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
        {
            i += 1;
            continue;
        }

        let indent = leading_spaces(line);
        if let Some((kind, name)) = match_brace_decl(trimmed, lang, indent) {
            let start_line = i + 1;
            // Collect signature up to opening '{' or ';'
            let mut sig = trimmed.to_string();
            let mut j = i;
            while !lines[j].contains('{')
                && !lines[j].trim_end().ends_with(';')
                && j + 1 < lines.len()
                && j - i < 8
            {
                j += 1;
                sig.push(' ');
                sig.push_str(lines[j].trim());
            }
            if let Some(brace_pos) = sig.find('{') {
                sig = sig[..brace_pos].trim_end().to_string();
            }

            // Find matching closing '}' ignoring strings and comments
            let mut brace_depth: i32 = 0;
            let mut seen_open = false;
            let mut scan_block_comment = false;
            let mut end_line = j + 1;
            for (k, l) in lines.iter().enumerate().skip(i) {
                let (delta, opened) = scan_braces_in_line(l, &mut scan_block_comment);
                if opened {
                    seen_open = true;
                }
                brace_depth += delta;
                if seen_open && brace_depth <= 0 {
                    end_line = k + 1;
                    break;
                }
                let lt = l.trim();
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

const CONTROL_FLOW_KEYWORDS: &[&str] = &[
    "if", "else", "for", "while", "switch", "catch", "return", "throw", "new", "delete", "typeof",
    "await", "yield", "case", "default", "try", "finally", "do", "match", "loop", "synchronized",
    "using", "lock", "foreach",
];

fn is_valid_ident(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_alphabetic() || first == '_' || first == '$') {
        return false;
    }
    chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        && !CONTROL_FLOW_KEYWORDS.contains(&s)
}

fn match_ts_arrow_or_method(trimmed: &str, indent: usize) -> Option<(&'static str, String)> {
    // 1. Arrow / function expression: `[export] const|let|var Name = [async] (...) =>` or `= function`
    let mut s = trimmed;
    for prefix in ["export default ", "export ", "declare "] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.trim_start();
        }
    }
    for kw in ["const ", "let ", "var "] {
        if let Some(rest) = s.strip_prefix(kw) {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                .collect();
            if is_valid_ident(&name) {
                let after = rest[name.len()..].trim_start();
                if let Some(eq_pos) = after.find('=') {
                    let rhs = after[eq_pos + 1..].trim_start();
                    // Avoid `==` or `===`
                    if !rhs.starts_with('=')
                        && (rhs.contains("=>")
                            || rhs.starts_with("async (")
                            || rhs.starts_with("async(")
                            || rhs.starts_with("function")
                            || rhs.starts_with("React.memo")
                            || rhs.starts_with("memo(")
                            || rhs.starts_with("forwardRef("))
                    {
                        return Some(("const", name));
                    }
                }
            }
        }
    }

    // 2. Class method inside indented class body: `[public|private|protected|static|readonly|async|get|set] name(...)`
    if indent > 0 && trimmed.contains('(') && !trimmed.contains(" = ") {
        let mut ms = trimmed;
        for modifier in [
            "public ",
            "private ",
            "protected ",
            "static ",
            "readonly ",
            "override ",
            "abstract ",
            "async ",
            "get ",
            "set ",
        ] {
            while let Some(rest) = ms.strip_prefix(modifier) {
                ms = rest.trim_start();
            }
        }
        let name: String = ms
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
            .collect();
        if is_valid_ident(&name) {
            let after = ms[name.len()..].trim_start();
            let starts_params = after.starts_with('(')
                || (after.starts_with('<') && after.contains(">("));
            if starts_params && (trimmed.ends_with('{') || trimmed.ends_with(')')) {
                return Some(("method", name));
            }
        }
    }

    None
}

fn match_cstyle_method(trimmed: &str) -> Option<(&'static str, String)> {
    // Must look like a function/method declaration with `(` and ending in `{` or `)` (not a call ending in `;`)
    if !trimmed.contains('(')
        || trimmed.ends_with(';')
        || trimmed.contains(" = ")
        || trimmed.starts_with('#')
    {
        return None;
    }
    let before_paren = trimmed.split('(').next()?.trim_end();
    // Handle generics like `void Foo<T>(...)`
    let cleaned = if let Some(lt) = before_paren.rfind('<') {
        if before_paren.ends_with('>') {
            before_paren[..lt].trim_end()
        } else {
            before_paren
        }
    } else {
        before_paren
    };

    let tokens: Vec<&str> = cleaned.split_whitespace().collect();
    if tokens.len() < 2 {
        return None;
    }
    let last = tokens.last()?.trim_start_matches('*').trim_start_matches('&');
    // Allow C++ `ClassName::method`
    let name_part = last.rsplit("::").next().unwrap_or(last);
    let name_clean = name_part.trim_start_matches('~');
    if is_valid_ident(name_clean) && !CONTROL_FLOW_KEYWORDS.contains(&tokens[0]) {
        return Some(("fn", last.to_string()));
    }
    None
}

fn match_brace_decl(trimmed: &str, lang: Lang, indent: usize) -> Option<(&'static str, String)> {
    // Strip common visibility / modifiers
    let mut s = trimmed;
    for prefix in [
        "pub(crate) ",
        "pub(super) ",
        "pub ",
        "export default ",
        "export ",
        "declare ",
        "async ",
        "unsafe ",
        "extern ",
        "inline ",
        "final ",
        "abstract ",
        "sealed ",
        "data ",
        "open ",
        "suspend ",
        "const ",
        "static ",
        "public ",
        "private ",
        "protected ",
        "internal ",
        "fileprivate ",
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
        Lang::Go => &[("func ", "func"), ("type ", "type")],
        Lang::CStyle => &[
            ("class ", "class"),
            ("struct ", "struct"),
            ("interface ", "interface"),
            ("enum ", "enum"),
            ("protocol ", "protocol"),
            ("extension ", "extension"),
            ("actor ", "actor"),
            ("func ", "func"), // Swift
            ("fun ", "fun"),   // Kotlin
            ("record ", "record"),
            ("namespace ", "namespace"),
        ],
        _ => &[],
    };

    for (kw, kind) in keywords {
        if let Some(rest) = s.strip_prefix(kw) {
            let rest = rest.trim_start();
            // Handle Go receiver `func (r *Receiver) Name(...)` or Rust `impl<T> Type`
            let after_generics_or_recv = if rest.starts_with('(') {
                rest.find(')')
                    .map(|p| rest[p + 1..].trim_start())
                    .unwrap_or(rest)
            } else if rest.starts_with('<') {
                rest.find('>')
                    .map(|p| rest[p + 1..].trim_start())
                    .unwrap_or(rest)
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

    if lang == Lang::TsJs {
        return match_ts_arrow_or_method(trimmed, indent);
    }
    if lang == Lang::CStyle {
        return match_cstyle_method(trimmed);
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
            if (1..=6).contains(&hashes) && t.chars().nth(hashes) == Some(' ') {
                let title = t[hashes..].trim().to_string();
                symbols.push(SymbolSpan {
                    kind: "heading",
                    name: title,
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
    fn test_python_symbols_decorators_and_unindented_docstring() {
        let src = r#"
class TokenAnalyzer:
    """Docstring that should not break parsing."""

    @staticmethod
    def build_sql():
        q = """
SELECT *
FROM sessions
"""
        return q

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
        assert_eq!(syms[1].name, "build_sql");
        // Decorator line should be included and unindented SQL must NOT truncate build_sql!
        assert_eq!(syms[1].start_line, 5);
        assert_eq!(syms[1].end_line, 11);
        assert_eq!(syms[2].name, "compute_metrics");
        assert_eq!(syms[3].name, "helper_fn");

        let enc = enclosing_symbol(&syms, 17).expect("should find enclosing symbol");
        assert_eq!(enc.name, "compute_metrics");
    }

    #[test]
    fn test_rust_symbols_with_braces_in_strings() {
        let src = r#"
pub struct Config {
    pub limit: usize,
}

impl Config {
    pub fn format_json(&self) -> String {
        let s = "{ \"open_only\": true";
        let c = '}';
        format!("{}{}", s, c)
    }
}
"#;
        let syms = extract_symbols(src, Lang::Rust);
        assert_eq!(syms.len(), 3);
        assert_eq!(syms[0].name, "Config");
        assert_eq!(syms[2].name, "format_json");
        assert_eq!(syms[2].end_line, 11);
    }

    #[test]
    fn test_ts_arrows_and_class_methods() {
        let src = r#"
export const Dashboard = ({ id }: Props) => {
  if (id === "") {
    return null;
  }
  return id;
};

const fetchItems = async (limit: number) => {
  return limit;
};

export class ApiClient {
  async getUser(id: string) {
    return id;
  }
}
"#;
        let syms = extract_symbols(src, Lang::TsJs);
        let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["Dashboard", "fetchItems", "ApiClient", "getUser"]);
        assert_eq!(syms[0].end_line, 7);
    }

    #[test]
    fn test_cstyle_java_kotlin_methods() {
        let src = r#"
public class OrderService {
    public static int calculateTotal(int a, int b) {
        if (a < 0) {
            return 0;
        }
        return a + b;
    }
}
"#;
        let syms = extract_symbols(src, Lang::CStyle);
        let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["OrderService", "calculateTotal"]);
        assert_eq!(syms[1].end_line, 8);
    }
}
