//! `read_code_sketch` — zoom level 1.5, between `read_code_skeleton` (signatures
//! only) and `read_code_body` (full source).
//!
//! Given skeleton IDs (`kind:start-end`), it returns each symbol's *control-flow
//! sketch*: the signature, branches/loops (`if`/`for`/`while`/`match`/…), block
//! delimiters and lines that perform a function call are kept verbatim; runs of
//! pure-data lines (simple assignments, literals, struct/array initialisers) are
//! collapsed into a single `… N lines …` placeholder. Typically 60–70% smaller
//! than the full body while preserving what the code *does*.

use std::path::Path;

use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::fs::estimate_tokens;
use crate::security::safe_path;

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ReadCodeSketchParams {
    /// Root-relative path to the code file.
    pub path: String,
    /// Skeleton IDs from read_code_skeleton (e.g. 'function:10-25').
    pub ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SketchItem {
    pub id: String,
    pub sketch: String,
    pub original_lines: usize,
    pub sketch_lines: usize,
}

#[derive(Debug, Serialize)]
pub struct ReadCodeSketchResult {
    pub items: Vec<SketchItem>,
    pub token_count: usize,
}

pub fn read_code_sketch(
    root: &Path,
    params: ReadCodeSketchParams,
) -> anyhow::Result<ReadCodeSketchResult> {
    let path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&path)?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let comment = line_comment_token(ext);
    let lines: Vec<&str> = content.lines().collect();

    let kw_re = keyword_regex();
    let call_re = Regex::new(r"[A-Za-z_][A-Za-z0-9_]*\s*\(").unwrap();

    let mut items = Vec::new();
    for id in &params.ids {
        match parse_range(id, lines.len()) {
            Ok((start, end)) => {
                let body = &lines[start..end];
                let sketch = sketch_block(body, comment, &kw_re, &call_re);
                let sketch_lines = sketch.lines().count();
                items.push(SketchItem {
                    id: id.clone(),
                    sketch,
                    original_lines: body.len(),
                    sketch_lines,
                });
            }
            Err(msg) => items.push(SketchItem {
                id: id.clone(),
                sketch: msg,
                original_lines: 0,
                sketch_lines: 0,
            }),
        }
    }

    let json = serde_json::to_string(&items).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadCodeSketchResult { items, token_count })
}

/// Parse a `kind:start-end` skeleton ID into a 0-based half-open line range.
fn parse_range(id: &str, total: usize) -> Result<(usize, usize), String> {
    let parts: Vec<&str> = id.rsplitn(2, ':').collect();
    if parts.len() == 2 {
        let range: Vec<&str> = parts[0].splitn(2, '-').collect();
        if range.len() == 2
            && let (Ok(start), Ok(end)) = (range[0].parse::<usize>(), range[1].parse::<usize>())
        {
            let start = start.saturating_sub(1);
            let end = end.min(total);
            if start >= end {
                return Err(format!(
                    "Error: id '{id}' is out of range (file has {total} lines). Re-run read_code_skeleton for fresh ids."
                ));
            }
            return Ok((start, end));
        }
    }
    Err(format!("Error: invalid id '{id}'"))
}

fn sketch_block(lines: &[&str], comment: &str, kw_re: &Regex, call_re: &Regex) -> String {
    let n = lines.len();
    let keep: Vec<bool> = lines
        .iter()
        .enumerate()
        .map(|(i, line)| i == 0 || is_structural(line, comment, kw_re, call_re))
        .collect();

    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < n {
        if keep[i] {
            out.push(lines[i].to_string());
            i += 1;
        } else {
            let from = i;
            while i < n && !keep[i] {
                i += 1;
            }
            let count = i - from;
            let indent: String = lines[from].chars().take_while(|c| c.is_whitespace()).collect();
            let plural = if count == 1 { "" } else { "s" };
            out.push(format!("{indent}{comment} … {count} line{plural} …"));
        }
    }
    out.join("\n")
}

/// A line is "structural" if it carries control flow, opens/closes a block, or
/// performs a call. Pure-data lines (assignments, literals) are not.
fn is_structural(line: &str, comment: &str, kw_re: &Regex, call_re: &Regex) -> bool {
    let t = line.trim();
    if t.is_empty() {
        return false;
    }
    // Comment-only lines carry no behaviour — elide them.
    if !comment.is_empty() && t.starts_with(comment) {
        return false;
    }
    // Block delimiters / structural punctuation only (`}`, `})`, `);`, …).
    if t.chars().all(|c| "{}()[];,".contains(c)) {
        return true;
    }
    // Block openers and match arms.
    if t.ends_with('{') || t.ends_with(':') || t.contains("=>") {
        return true;
    }
    // Ruby / Lua block terminator.
    if t == "end" {
        return true;
    }
    if kw_re.is_match(t) {
        return true;
    }
    call_re.is_match(t)
}

fn keyword_regex() -> Regex {
    Regex::new(
        r"\b(if|else|elif|for|while|loop|match|switch|case|when|try|catch|except|finally|return|break|continue|guard|defer|select|throw|throws|raise|yield|do|goto|await)\b",
    )
    .unwrap()
}

fn line_comment_token(ext: &str) -> &'static str {
    match ext {
        "py" | "rb" | "sh" | "yaml" | "yml" | "toml" | "pyi" => "#",
        "lua" | "sql" => "--",
        _ => "//",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sketch(src: &str, ext: &str) -> String {
        let lines: Vec<&str> = src.lines().collect();
        sketch_block(
            &lines,
            line_comment_token(ext),
            &keyword_regex(),
            &Regex::new(r"[A-Za-z_][A-Za-z0-9_]*\s*\(").unwrap(),
        )
    }

    #[test]
    fn collapses_data_keeps_control_flow_and_calls() {
        let src = "pub fn process(items: &[Item]) -> usize {\n    let mut total = 0;\n    let base = 10;\n    let factor = 2;\n    if items.is_empty() {\n        return 0;\n    }\n    for item in items {\n        total += item.value;\n    }\n    log_result(total);\n    total\n}";
        let out = sketch(src, "rs");
        // signature kept
        assert!(out.contains("pub fn process"));
        // control flow kept
        assert!(out.contains("if items.is_empty()"));
        assert!(out.contains("return 0;"));
        assert!(out.contains("for item in items"));
        // call kept
        assert!(out.contains("log_result(total);"));
        // data lines collapsed
        assert!(out.contains("… 3 lines …")); // the three `let` lines
        // pure-data assignment without a call collapsed too
        assert!(!out.contains("total += item.value;"));
        // overall smaller
        assert!(out.lines().count() < src.lines().count());
    }

    #[test]
    fn single_elided_line_uses_singular() {
        let src = "fn f() {\n    let x = 1;\n    call_it();\n}";
        let out = sketch(src, "rs");
        assert!(out.contains("… 1 line …"));
        assert!(out.contains("call_it();"));
    }

    #[test]
    fn python_uses_hash_placeholder() {
        let src = "def f():\n    a = 1\n    b = 2\n    return a + b";
        let out = sketch(src, "py");
        assert!(out.contains("# … 2 lines …"));
        assert!(out.contains("return a + b"));
    }

    #[test]
    fn keyword_match_is_word_bounded() {
        // "format" contains "for" but must not be treated as a loop.
        let kw = keyword_regex();
        assert!(!kw.is_match("let s = format_value;"));
        assert!(kw.is_match("for x in xs {"));
    }

    #[test]
    fn invalid_id_reports_error() {
        let err = parse_range("garbage", 100).unwrap_err();
        assert!(err.contains("invalid id"));
        let oob = parse_range("function:500-600", 100).unwrap_err();
        assert!(oob.contains("out of range"));
    }

    #[test]
    fn comment_only_lines_are_elided() {
        let src = "fn f() {\n    // this explains nothing structural\n    do_work();\n}";
        let out = sketch(src, "rs");
        assert!(!out.contains("explains nothing"));
        assert!(out.contains("do_work();"));
    }
}
