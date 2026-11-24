//! Core lossless-by-default output compressor:
//! 1. Strips ANSI escape sequences and carriage-return progress overwrites.
//! 2. Collapses consecutive duplicate lines with `[xN]`.
//! 3. Elides passing test / build progress lines while preserving 100% of errors,
//!    tracebacks, warnings, and summary lines.
//! 4. Returns `(compressed_text, lines_elided)` so the caller can spill the raw
//!    output to `.eai/logs/<id>.log` and append a one-line recovery pointer.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressResult {
    pub output: String,
    pub total_lines: usize,
    pub kept_lines: usize,
    pub elided_lines: usize,
}

/// Strip ANSI CSI/OSC sequences and resolve `\r` progress overwrites within a line.
pub fn clean_ansi_and_cr(input: &str) -> String {
    let mut cleaned_lines = Vec::new();
    for raw_line in input.split('\n') {
        let line_no_cr = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        // If line contains internal `\r` (spinner / progress bar overwrite), keep only the final segment
        let final_seg = line_no_cr.rsplit('\r').next().unwrap_or(line_no_cr);
        cleaned_lines.push(strip_ansi_line(final_seg));
    }
    cleaned_lines.join("\n")
}

fn strip_ansi_line(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(&nc) = chars.peek() {
                    chars.next();
                    if ('@'..='~').contains(&nc) {
                        break;
                    }
                }
            } else if chars.peek() == Some(&']') {
                chars.next();
                while let Some(&nc) = chars.peek() {
                    chars.next();
                    if nc == '\x07' {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Classify whether a line is a routine passing test or build-progress line that can be elided
/// when the full log is spilled to disk.
fn is_elidable_passing_or_progress(trimmed: &str) -> bool {
    if trimmed.is_empty() {
        return false;
    }
    // Never elide lines that mention failure, error, warning, traceback, panic, or assertion
    let lower = trimmed.to_ascii_lowercase();
    for keep_kw in [
        "error",
        "fail",
        "warn",
        "traceback",
        "exception",
        "panic",
        "assert",
        "summary",
        "short test summary",
        "fatal",
        "segmentation",
    ] {
        if lower.contains(keep_kw) {
            return false;
        }
    }

    // Pytest verbose passing line: `tests/foo.py::test_bar PASSED [ 15%]`
    if trimmed.contains("::") && (trimmed.contains(" PASSED") || trimmed.ends_with("PASSED")) {
        return true;
    }
    // Python unittest verbose passing line: `test_foo (module.TestBar) ... ok`
    if trimmed.starts_with("test_") && trimmed.ends_with(" ... ok") {
        return true;
    }
    // Rust cargo test passing line: `test foo::bar ... ok`
    if trimmed.starts_with("test ") && trimmed.ends_with(" ... ok") {
        return true;
    }
    // Go test verbose passing lines: `=== RUN   TestFoo` / `--- PASS: TestFoo (0.00s)`
    if trimmed.starts_with("=== RUN") || trimmed.starts_with("--- PASS:") {
        return true;
    }
    // Gradle / Maven / Flutter / Docker / Dotnet / Mocha / Vitest routine progress lines
    if trimmed.starts_with("> Task ") && trimmed.ends_with("UP-TO-DATE") {
        return true;
    }
    if trimmed.starts_with("[INFO] Downloading ") || trimmed.starts_with("[INFO] Downloaded ") {
        return true;
    }
    if trimmed.starts_with('#') && (trimmed.ends_with(" CACHED") || trimmed.contains(" DONE ")) {
        return true;
    }
    if trimmed.starts_with("Passed ") && trimmed.contains('[') && trimmed.ends_with(']') {
        return true;
    }
    // Cargo / npm / pip / pnpm / docker build download & compile progress
    for prefix in [
        "Downloading ",
        "Downloaded ",
        "Compiling ",
        "Checking ",
        "Fresh ",
        "Locking ",
        "Unpacking ",
        "Updating crates.io",
        "Requirement already satisfied:",
        "Using cached ",
        "Collecting ",
        "Resolving ",
        "Resolved ",
        "PASS ",
        "✓ ",
        "√ ",
        "✔ ",
        "ok   ",
        "ok\t",
    ] {
        if trimmed.starts_with(prefix) {
            return true;
        }
    }

    false
}

pub fn compress_stream(raw: &str, max_tail_lines: usize) -> CompressResult {
    let cleaned = clean_ansi_and_cr(raw);
    let lines: Vec<&str> = cleaned.lines().collect();
    let total_lines = lines.len();

    let mut deduped: Vec<String> = Vec::new();
    let mut elided_lines = 0usize;

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // 1. Elide routine passing test / build progress lines
        if is_elidable_passing_or_progress(trimmed) {
            elided_lines += 1;
            i += 1;
            continue;
        }

        // 2. Collapse consecutive identical lines
        let mut repeat_count = 1usize;
        while i + repeat_count < lines.len() && lines[i + repeat_count] == line {
            repeat_count += 1;
        }
        if repeat_count > 1 && !trimmed.is_empty() {
            deduped.push(format!("{} [x{}]", line, repeat_count));
            elided_lines += repeat_count - 1;
        } else {
            deduped.push(line.to_string());
        }
        i += repeat_count;
    }

    // 3. If output is still very long (> max_tail_lines), keep head (30%) + tail (70%)
    if max_tail_lines > 20 && deduped.len() > max_tail_lines {
        let head_n = max_tail_lines * 3 / 10;
        let tail_n = max_tail_lines - head_n;
        let middle_dropped = deduped.len() - head_n - tail_n;
        elided_lines += middle_dropped;

        let mut compact = Vec::with_capacity(max_tail_lines + 1);
        compact.extend_from_slice(&deduped[..head_n]);
        compact.push(format!("... [{} middle lines elided] ...", middle_dropped));
        compact.extend_from_slice(&deduped[deduped.len() - tail_n..]);
        deduped = compact;
    }

    let kept_lines = deduped.len();
    CompressResult {
        output: deduped.join("\n"),
        total_lines,
        kept_lines,
        elided_lines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi_and_progress_cr() {
        let raw = "Downloading 10%\rDownloading 50%\r\x1b[32mDownloaded 100%\x1b[0m\nDone";
        let cleaned = clean_ansi_and_cr(raw);
        assert_eq!(cleaned, "Downloaded 100%\nDone");
    }

    #[test]
    fn test_pytest_passing_elided_failures_kept() {
        let raw = "\
============================= test session starts ==============================
tests/test_a.py::test_1 PASSED                                           [ 33%]
tests/test_a.py::test_2 PASSED                                           [ 66%]
tests/test_a.py::test_3 FAILED                                           [100%]
=================================== FAILURES ===================================
E   AssertionError: expected 42 got 41
========================= 1 failed, 2 passed in 0.12s =========================";
        let res = compress_stream(raw, 200);
        assert_eq!(res.elided_lines, 2);
        assert!(res.output.contains("test_3 FAILED"));
        assert!(res.output.contains("AssertionError: expected 42 got 41"));
        assert!(!res.output.contains("test_1 PASSED"));
    }

    #[test]
    fn test_go_and_vitest_elided_errors_kept() {
        let raw = "\
=== RUN   TestAlpha
--- PASS: TestAlpha (0.01s)
=== RUN   TestBeta
--- FAIL: TestBeta (0.02s)
    beta_test.go:18: expected 5 got 4
PASS src/utils.test.ts
✓ formats numbers (2 ms)
FAIL";
        let res = compress_stream(raw, 200);
        assert!(res.elided_lines >= 5);
        assert!(res.output.contains("--- FAIL: TestBeta"));
        assert!(res.output.contains("expected 5 got 4"));
        assert!(!res.output.contains("--- PASS: TestAlpha"));
    }

    #[test]
    fn test_duplicate_lines_collapsed() {
        let raw = "retry connection...\nretry connection...\nretry connection...\nconnected!";
        let res = compress_stream(raw, 200);
        assert!(res.output.contains("retry connection... [x3]"));
        assert_eq!(res.elided_lines, 2);
    }
}
