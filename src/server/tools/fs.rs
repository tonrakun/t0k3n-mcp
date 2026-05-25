use ignore::WalkBuilder;
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt::Write as FmtWrite;
use std::path::Path;

use crate::security::safe_path;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadDirectoryTreeParams {
    #[schemars(description = "Root-relative path to start from (omit for project root)")]
    pub path: Option<String>,
    #[schemars(description = "Maximum depth (default: 3, max: 10)")]
    pub depth: Option<usize>,
}

pub struct DirectoryTreeResult {
    pub tree: String,
    pub token_count: usize,
}

pub fn read_directory_tree(root: &Path, params: ReadDirectoryTreeParams) -> anyhow::Result<DirectoryTreeResult> {
    let start = if let Some(ref p) = params.path {
        safe_path(root, p)?
    } else {
        root.to_path_buf()
    };
    let depth = params.depth.unwrap_or(3).min(10);

    let mut out = String::new();
    let _ = write!(out, "./\n");

    let walker = WalkBuilder::new(&start)
        .max_depth(Some(depth))
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .build();

    let start_depth = start.components().count();

    for entry in walker.flatten() {
        let path = entry.path();
        if path == start {
            continue;
        }
        let depth_diff = path.components().count() - start_depth;
        if depth_diff == 0 {
            continue;
        }
        let indent = "│   ".repeat(depth_diff.saturating_sub(1));
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let is_dir = path.is_dir();
        let suffix = if is_dir { "/" } else { "" };
        let _ = writeln!(out, "{}├── {}{}", indent, name, suffix);
    }

    let token_count = estimate_tokens(&out);
    Ok(DirectoryTreeResult { tree: out, token_count })
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SearchFileParams {
    #[schemars(description = "Root-relative file path to search")]
    pub path: String,
    #[schemars(description = "Search query (regex supported)")]
    pub query: String,
    #[schemars(description = "Lines of context before/after match (default: 2, max: 10)")]
    pub context_lines: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchMatch {
    pub line: usize,
    pub content: String,
    pub context: Vec<String>,
}

pub struct SearchFileResult {
    pub matches: Vec<SearchMatch>,
    pub token_count: usize,
}

pub fn search_file(root: &Path, params: SearchFileParams) -> anyhow::Result<SearchFileResult> {
    let path = safe_path(root, &params.path)?;
    if path.is_dir() {
        anyhow::bail!("'{}' is a directory, not a file", params.path);
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", params.path, e))?;
    let lines: Vec<&str> = content.lines().collect();
    let ctx = params.context_lines.unwrap_or(2).min(10);

    let re = Regex::new(&params.query)
        .map_err(|e| anyhow::anyhow!("Invalid regex '{}': {}", params.query, e))?;
    let mut matches = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        if re.is_match(line) {
            let start = i.saturating_sub(ctx);
            let end = (i + ctx + 1).min(lines.len());
            let context: Vec<String> = (start..end)
                .filter(|&j| j != i)
                .map(|j| format!("{}: {}", j + 1, lines[j]))
                .collect();
            matches.push(SearchMatch {
                line: i + 1,
                content: line.to_string(),
                context,
            });
        }
    }

    let json = serde_json::to_string(&matches).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(SearchFileResult { matches, token_count })
}

pub fn estimate_tokens(text: &str) -> usize {
    // CJK/Japanese chars are 3 UTF-8 bytes but ~1 token each (len/4 underestimates 40-60%).
    // Split by character class for better accuracy across Latin, CJK, and mixed content.
    let mut ascii = 0usize;
    let mut cjk = 0usize;
    let mut other = 0usize;
    for ch in text.chars() {
        let cp = ch as u32;
        if ch.is_ascii() {
            ascii += 1;
        } else if matches!(cp, 0x3000..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2FA1F) {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    // ASCII ~4 chars/token, CJK ~1 char/token, other ~2 chars/token
    (ascii / 4 + cjk + other / 2).max(1)
}
