use std::collections::HashSet;
use std::path::Path;

use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::security::{rel_display, safe_path};
use super::fs::estimate_tokens;

// ─── read_log_tail ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadLogTailParams {
    #[schemars(description = "Root-relative path to a log file.")]
    pub path: String,
    #[schemars(description = "Max lines to return (default: 100, max: 1000).")]
    pub lines: Option<usize>,
    #[schemars(description = "Filter by log level: ERROR, WARN, INFO, DEBUG (case-insensitive). Omit for all levels.")]
    pub level: Option<String>,
    #[schemars(description = "Additional filter regex pattern. Only matching lines are returned.")]
    pub pattern: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LogLevelCounts {
    pub error: usize,
    pub warn: usize,
    pub info: usize,
    pub debug: usize,
    pub other: usize,
}

#[derive(Debug, Serialize)]
pub struct ReadLogTailResult {
    pub path: String,
    pub total_lines: usize,
    pub returned_lines: usize,
    pub level_counts: LogLevelCounts,
    pub lines: Vec<String>,
    pub token_count: usize,
}

pub fn read_log_tail(root: &Path, params: ReadLogTailParams) -> anyhow::Result<ReadLogTailResult> {
    let file_path = safe_path(root, &params.path)?;
    let max_lines = params.lines.unwrap_or(100).min(1000);

    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| anyhow::anyhow!("ファイル読み取り失敗: {e}"))?;

    let all_lines: Vec<&str> = content.lines().collect();
    let total = all_lines.len();

    let level_filter = params.level.as_deref().map(|l| l.to_uppercase());
    let pattern_re = params.pattern.as_deref()
        .map(|p| Regex::new(p))
        .transpose()
        .map_err(|e| anyhow::anyhow!("無効な正規表現: {e}"))?;

    let mut counts = LogLevelCounts { error: 0, warn: 0, info: 0, debug: 0, other: 0 };
    for line in &all_lines {
        match detect_log_level(line) {
            "ERROR" => counts.error += 1,
            "WARN"  => counts.warn  += 1,
            "INFO"  => counts.info  += 1,
            "DEBUG" => counts.debug += 1,
            _       => counts.other += 1,
        }
    }

    // Scan from the tail; scan up to 10× max_lines to have enough after filtering
    let scan_start = total.saturating_sub(max_lines.saturating_mul(10).max(max_lines));
    let filtered: Vec<String> = all_lines[scan_start..]
        .iter()
        .filter(|line| {
            if let Some(ref lvl) = level_filter {
                if detect_log_level(line) != lvl.as_str() { return false; }
            }
            if let Some(ref re) = pattern_re {
                if !re.is_match(line) { return false; }
            }
            true
        })
        .map(|s| s.to_string())
        .collect();

    let result_lines: Vec<String> = filtered.into_iter().rev().take(max_lines).rev().collect();

    let rel = rel_display(root, &file_path);
    let token_count = estimate_tokens(&result_lines.join("\n"));

    Ok(ReadLogTailResult {
        path: rel,
        total_lines: total,
        returned_lines: result_lines.len(),
        level_counts: counts,
        lines: result_lines,
        token_count,
    })
}

fn detect_log_level(line: &str) -> &'static str {
    let u = line.to_uppercase();
    if u.contains("ERROR") || u.contains("CRITICAL") || u.contains("FATAL") { "ERROR" }
    else if u.contains("WARN") { "WARN" }
    else if u.contains("INFO")  { "INFO"  }
    else if u.contains("DEBUG") || u.contains("TRACE") { "DEBUG" }
    else { "OTHER" }
}

// ─── read_stack_trace ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadStackTraceParams {
    #[schemars(description = "Full stack trace text to parse (paste the complete error + trace).")]
    pub stack_trace: String,
    #[schemars(description = "Lines of context to show around each referenced line (default: 5).")]
    pub context_lines: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct StackTraceFrame {
    pub file: String,
    pub line: usize,
    pub function: Option<String>,
    pub source_context: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReadStackTraceResult {
    pub total_frames: usize,
    pub resolved_frames: usize,
    pub frames: Vec<StackTraceFrame>,
    pub token_count: usize,
}

pub fn read_stack_trace(root: &Path, params: ReadStackTraceParams) -> anyhow::Result<ReadStackTraceResult> {
    let context = params.context_lines.unwrap_or(5);
    let raw_frames = parse_stack_trace(&params.stack_trace);

    let mut frames = Vec::new();
    let mut resolved = 0;

    for (file, line, func) in &raw_frames {
        let source_context = try_read_context(root, file, *line, context);
        if source_context.is_some() { resolved += 1; }
        frames.push(StackTraceFrame {
            file: file.clone(),
            line: *line,
            function: func.clone(),
            source_context,
        });
    }

    let json = serde_json::to_string(&frames).unwrap_or_default();
    let token_count = estimate_tokens(&json);

    Ok(ReadStackTraceResult { total_frames: frames.len(), resolved_frames: resolved, frames, token_count })
}

fn parse_stack_trace(text: &str) -> Vec<(String, usize, Option<String>)> {
    let mut results: Vec<(String, usize, Option<String>)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // Python:  File "path/to/file.py", line 42, in fn_name
    let py = Regex::new(r#"File "([^"]+)", line (\d+)(?:, in (\S+))?"#).unwrap();
    // C#:      at Namespace.Class.Method() in /path/File.cs:line 42
    let cs = Regex::new(r"in ([^\r\n:]+\.cs):line (\d+)").unwrap();
    // JS/TS:   at fn (path/file.js:42:10) or at path/file.ts:42:10
    let js = Regex::new(r"\bat (?:([\w.<>$]+) \()?([\w./\\-]+\.[jt]sx?):(\d+)").unwrap();
    // Java:    at com.example.Class.method(File.java:42)
    let java = Regex::new(r"\bat [\w.$]+\((\w+\.java):(\d+)\)").unwrap();
    // Rust:    at src/main.rs:42  (also matches Go paths)
    let rs = Regex::new(r"\bat ([\w./\\-]+\.\w+):(\d+)").unwrap();
    // Go:      /path/to/file.go:42 +0x...
    let go = Regex::new(r"([\w./\\-]+\.go):(\d+)").unwrap();

    for line in text.lines() {
        macro_rules! push {
            ($file:expr, $lineno:expr, $func:expr) => {{
                let key = format!("{}:{}", $file, $lineno);
                if seen.insert(key) {
                    results.push(($file, $lineno, $func));
                }
            }};
        }

        if let Some(cap) = py.captures(line) {
            push!(cap[1].to_string(), cap[2].parse().unwrap_or(0), cap.get(3).map(|m| m.as_str().to_string()));
            continue;
        }
        if let Some(cap) = cs.captures(line) {
            push!(cap[1].to_string(), cap[2].parse().unwrap_or(0), None);
            continue;
        }
        if let Some(cap) = js.captures(line) {
            push!(cap[2].to_string(), cap[3].parse().unwrap_or(0), cap.get(1).map(|m| m.as_str().to_string()));
            continue;
        }
        if let Some(cap) = java.captures(line) {
            push!(cap[1].to_string(), cap[2].parse().unwrap_or(0), None);
            continue;
        }
        if let Some(cap) = rs.captures(line) {
            push!(cap[1].to_string(), cap[2].parse().unwrap_or(0), None);
            continue;
        }
        if let Some(cap) = go.captures(line) {
            push!(cap[1].to_string(), cap[2].parse().unwrap_or(0), None);
        }
    }

    results
}

fn try_read_context(root: &Path, file: &str, line: usize, context: usize) -> Option<String> {
    if line == 0 { return None; }

    let candidates = [root.join(file), std::path::PathBuf::from(file)];

    for path in &candidates {
        if let Ok(content) = std::fs::read_to_string(path) {
            let lines: Vec<&str> = content.lines().collect();
            if line > lines.len() { continue; }

            let from = line.saturating_sub(context + 1);
            let to = (line + context).min(lines.len());

            let ctx: Vec<String> = lines[from..to].iter().enumerate()
                .map(|(i, l)| {
                    let ln = from + i + 1;
                    if ln == line { format!(">{:4} | {}", ln, l) }
                    else          { format!(" {:4} | {}", ln, l) }
                })
                .collect();

            return Some(ctx.join("\n"));
        }
    }

    None
}
