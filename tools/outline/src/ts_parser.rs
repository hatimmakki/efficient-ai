//! True AST symbol extraction using tree-sitter for Python, Rust, TypeScript/TSX,
//! JavaScript/JSX, Go, and Java.

use crate::parser::SymbolSpan;
use std::path::Path;
use tree_sitter::{Language, Node, Parser};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TsLang {
    Python,
    Rust,
    TypeScript,
    Tsx,
    JavaScript,
    Go,
    Java,
}

impl TsLang {
    pub fn from_path(path: &Path) -> Option<Self> {
        match path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "py" | "pyi" => Some(Self::Python),
            "rs" => Some(Self::Rust),
            "ts" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            "js" | "jsx" | "mjs" | "cjs" => Some(Self::JavaScript),
            "go" => Some(Self::Go),
            "java" => Some(Self::Java),
            _ => None,
        }
    }

    fn language(self) -> Language {
        match self {
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Self::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Self::Go => tree_sitter_go::LANGUAGE.into(),
            Self::Java => tree_sitter_java::LANGUAGE.into(),
        }
    }
}

fn node_text<'a>(node: Node<'_>, src: &'a [u8]) -> &'a str {
    node.utf8_text(src).unwrap_or("")
}

fn compact_signature(node: Node<'_>, src: &[u8], lines: &[&str]) -> String {
    let start_row = node.start_position().row;
    let end_row = node.end_position().row;
    // Collect signature from start_row up to first '{' or ':' (Python) or max 4 lines
    let mut parts = Vec::new();
    let max_row = end_row.min(start_row + 4);
    for line in lines.iter().take(max_row + 1).skip(start_row) {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(brace) = t.find('{') {
            let before = t[..brace].trim_end();
            if !before.is_empty() {
                parts.push(before.to_string());
            }
            break;
        }
        parts.push(t.to_string());
        if t.ends_with(':') || t.ends_with(';') {
            break;
        }
    }
    let joined = parts.join(" ");
    if joined.is_empty() {
        node_text(node, src)
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string()
    } else if joined.len() > 140 {
        format!("{}...", &joined[..137])
    } else {
        joined
    }
}

#[allow(clippy::too_many_arguments)]
fn push_span(
    out: &mut Vec<SymbolSpan>,
    kind: &'static str,
    name: String,
    node: Node<'_>,
    sig_node: Node<'_>,
    depth: usize,
    src: &[u8],
    lines: &[&str],
) {
    if name.is_empty() {
        return;
    }
    let start_line = node.start_position().row + 1;
    let end_line = node.end_position().row + 1;
    let sig = compact_signature(sig_node, src, lines);
    out.push(SymbolSpan {
        kind,
        name,
        signature: sig,
        indent: depth * 2,
        start_line,
        end_line,
    });
}

fn walk_python(node: Node<'_>, src: &[u8], lines: &[&str], depth: usize, out: &mut Vec<SymbolSpan>) {
    let kind = node.kind();
    match kind {
        "decorated_definition" => {
            if let Some(def) = node.child_by_field_name("definition") {
                let sym_kind = if def.kind() == "class_definition" {
                    "class"
                } else {
                    "def"
                };
                let name = def
                    .child_by_field_name("name")
                    .map(|n| node_text(n, src).to_string())
                    .unwrap_or_default();
                push_span(out, sym_kind, name, node, def, depth, src, lines);
                // Recurse into class body to pick up methods
                if let Some(body) = def.child_by_field_name("body") {
                    let mut cursor = body.walk();
                    for child in body.children(&mut cursor) {
                        walk_python(child, src, lines, depth + 1, out);
                    }
                }
                return;
            }
        }
        "class_definition" | "function_definition" => {
            let sym_kind = if kind == "class_definition" {
                "class"
            } else {
                "def"
            };
            let name = node
                .child_by_field_name("name")
                .map(|n| node_text(n, src).to_string())
                .unwrap_or_default();
            push_span(out, sym_kind, name, node, node, depth, src, lines);
            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    walk_python(child, src, lines, depth + 1, out);
                }
            }
            return;
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_python(child, src, lines, depth, out);
    }
}

fn walk_rust(node: Node<'_>, src: &[u8], lines: &[&str], depth: usize, out: &mut Vec<SymbolSpan>) {
    match node.kind() {
        "struct_item" | "enum_item" | "trait_item" | "function_item" | "type_item"
        | "mod_item" | "macro_definition" => {
            let sym_kind = match node.kind() {
                "struct_item" => "struct",
                "enum_item" => "enum",
                "trait_item" => "trait",
                "function_item" => "fn",
                "type_item" => "type",
                "mod_item" => "mod",
                _ => "macro",
            };
            let name = node
                .child_by_field_name("name")
                .map(|n| node_text(n, src).to_string())
                .unwrap_or_default();
            push_span(out, sym_kind, name, node, node, depth, src, lines);
            if matches!(node.kind(), "trait_item" | "mod_item") {
                if let Some(body) = node.child_by_field_name("body") {
                    let mut cursor = body.walk();
                    for child in body.children(&mut cursor) {
                        walk_rust(child, src, lines, depth + 1, out);
                    }
                }
            }
            return;
        }
        "impl_item" => {
            let name = node
                .child_by_field_name("type")
                .map(|n| node_text(n, src).to_string())
                .unwrap_or_else(|| "impl".to_string());
            push_span(out, "impl", name, node, node, depth, src, lines);
            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    walk_rust(child, src, lines, depth + 1, out);
                }
            }
            return;
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_rust(child, src, lines, depth, out);
    }
}

fn is_callable_initializer(node: Node<'_>) -> bool {
    match node.kind() {
        "arrow_function" | "function_expression" | "function" | "generator_function" => true,
        "call_expression" => {
            // React.memo(...), forwardRef(...), etc.
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "arguments" {
                    let mut ac = child.walk();
                    for arg in child.children(&mut ac) {
                        if matches!(
                            arg.kind(),
                            "arrow_function" | "function_expression" | "function"
                        ) {
                            return true;
                        }
                    }
                }
            }
            false
        }
        _ => false,
    }
}

fn walk_ts_js(node: Node<'_>, src: &[u8], lines: &[&str], depth: usize, out: &mut Vec<SymbolSpan>) {
    match node.kind() {
        "function_declaration"
        | "generator_function_declaration"
        | "class_declaration"
        | "abstract_class_declaration"
        | "interface_declaration"
        | "type_alias_declaration"
        | "enum_declaration" => {
            let sym_kind = match node.kind() {
                "function_declaration" | "generator_function_declaration" => "function",
                "class_declaration" | "abstract_class_declaration" => "class",
                "interface_declaration" => "interface",
                "type_alias_declaration" => "type",
                _ => "enum",
            };
            let name = node
                .child_by_field_name("name")
                .map(|n| node_text(n, src).to_string())
                .unwrap_or_default();
            push_span(out, sym_kind, name, node, node, depth, src, lines);
            if matches!(
                node.kind(),
                "class_declaration" | "abstract_class_declaration"
            ) {
                if let Some(body) = node.child_by_field_name("body") {
                    let mut cursor = body.walk();
                    for child in body.children(&mut cursor) {
                        walk_ts_js(child, src, lines, depth + 1, out);
                    }
                }
            }
            return;
        }
        "method_definition" => {
            let name = node
                .child_by_field_name("name")
                .map(|n| node_text(n, src).to_string())
                .unwrap_or_default();
            push_span(out, "method", name, node, node, depth, src, lines);
            return;
        }
        "lexical_declaration" | "variable_declaration" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "variable_declarator" {
                    let has_fn = child
                        .child_by_field_name("value")
                        .map(is_callable_initializer)
                        .unwrap_or(false);
                    if has_fn {
                        let name = child
                            .child_by_field_name("name")
                            .map(|n| node_text(n, src).to_string())
                            .unwrap_or_default();
                        // Use parent export_statement span if wrapped in `export const Foo = ...`
                        let span_node = node
                            .parent()
                            .filter(|p| p.kind() == "export_statement")
                            .unwrap_or(node);
                        push_span(out, "const", name, span_node, span_node, depth, src, lines);
                    }
                }
            }
            return;
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_ts_js(child, src, lines, depth, out);
    }
}

fn walk_go(node: Node<'_>, src: &[u8], lines: &[&str], depth: usize, out: &mut Vec<SymbolSpan>) {
    match node.kind() {
        "function_declaration" | "method_declaration" => {
            let name = node
                .child_by_field_name("name")
                .map(|n| node_text(n, src).to_string())
                .unwrap_or_default();
            push_span(out, "func", name, node, node, depth, src, lines);
            return;
        }
        "type_declaration" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "type_spec" {
                    let name = child
                        .child_by_field_name("name")
                        .map(|n| node_text(n, src).to_string())
                        .unwrap_or_default();
                    push_span(out, "type", name, node, node, depth, src, lines);
                }
            }
            return;
        }
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_go(child, src, lines, depth, out);
    }
}

fn walk_java(node: Node<'_>, src: &[u8], lines: &[&str], depth: usize, out: &mut Vec<SymbolSpan>) {
    match node.kind() {
        "class_declaration"
        | "interface_declaration"
        | "enum_declaration"
        | "record_declaration" => {
            let sym_kind = match node.kind() {
                "class_declaration" => "class",
                "interface_declaration" => "interface",
                "enum_declaration" => "enum",
                _ => "record",
            };
            let name = node
                .child_by_field_name("name")
                .map(|n| node_text(n, src).to_string())
                .unwrap_or_default();
            push_span(out, sym_kind, name, node, node, depth, src, lines);
            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    walk_java(child, src, lines, depth + 1, out);
                }
            }
            return;
        }
        "method_declaration" | "constructor_declaration" => {
            let name = node
                .child_by_field_name("name")
                .map(|n| node_text(n, src).to_string())
                .unwrap_or_default();
            push_span(out, "method", name, node, node, depth, src, lines);
            return;
        }
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_java(child, src, lines, depth, out);
    }
}

/// Extract AST-accurate symbol spans using tree-sitter for supported languages.
pub fn extract_with_treesitter(source: &str, path: &Path) -> Option<Vec<SymbolSpan>> {
    let ts_lang = TsLang::from_path(path)?;
    let mut parser = Parser::new();
    parser.set_language(&ts_lang.language()).ok()?;
    let tree = parser.parse(source, None)?;
    let lines: Vec<&str> = source.lines().collect();
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let root = tree.root_node();

    match ts_lang {
        TsLang::Python => walk_python(root, bytes, &lines, 0, &mut out),
        TsLang::Rust => walk_rust(root, bytes, &lines, 0, &mut out),
        TsLang::TypeScript | TsLang::Tsx | TsLang::JavaScript => {
            walk_ts_js(root, bytes, &lines, 0, &mut out)
        }
        TsLang::Go => walk_go(root, bytes, &lines, 0, &mut out),
        TsLang::Java => walk_java(root, bytes, &lines, 0, &mut out),
    }

    Some(out)
}
