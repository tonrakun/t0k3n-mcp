use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::security::safe_path;
use super::fs::estimate_tokens;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadCodeSkeletonParams {
    #[schemars(description = "Root-relative path to the code file")]
    pub path: String,
    #[schemars(description = "Include block-level constructs (if/for/etc) - default false")]
    pub include_blocks: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkeletonItem {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub signature: String,
    pub start_line: usize,
    pub end_line: usize,
}

pub struct ReadCodeSkeletonResult {
    pub skeleton: Vec<SkeletonItem>,
    pub token_count: usize,
}

pub fn read_code_skeleton(root: &Path, params: ReadCodeSkeletonParams) -> anyhow::Result<ReadCodeSkeletonResult> {
    let path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&path)?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let skeleton = parse_skeleton(&content, ext);
    let json = serde_json::to_string(&skeleton).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadCodeSkeletonResult { skeleton, token_count })
}

fn parse_skeleton(content: &str, ext: &str) -> Vec<SkeletonItem> {
    match ext {
        "rs" => parse_rust(content),
        "py" => parse_python(content),
        "js" | "jsx" | "ts" | "tsx" => parse_js_ts(content),
        "go" => parse_go(content),
        _ => parse_generic(content),
    }
}

fn parse_rust(content: &str) -> Vec<SkeletonItem> {
    let lines: Vec<&str> = content.lines().collect();
    let mut items = Vec::new();

    // Match fn, struct, enum, impl, trait, type, const, static, mod
    let patterns = [
        (Regex::new(r"^(\s*)(pub\s+)?(async\s+)?fn\s+(\w+)").unwrap(), "function"),
        (Regex::new(r"^(\s*)(pub\s+)?struct\s+(\w+)").unwrap(), "struct"),
        (Regex::new(r"^(\s*)(pub\s+)?enum\s+(\w+)").unwrap(), "enum"),
        (Regex::new(r"^(\s*)(pub\s+)?trait\s+(\w+)").unwrap(), "trait"),
        (Regex::new(r"^(\s*)impl(\s*<[^>]*>)?\s+(\w+)").unwrap(), "impl"),
        (Regex::new(r"^(\s*)(pub\s+)?mod\s+(\w+)").unwrap(), "mod"),
    ];

    for (i, line) in lines.iter().enumerate() {
        for (re, kind) in &patterns {
            if let Some(cap) = re.captures(line) {
                let name = cap.get(cap.len() - 1).map(|m| m.as_str()).unwrap_or("").to_string();
                if name.is_empty() { continue; }
                let end_line = find_block_end(&lines, i);
                items.push(SkeletonItem {
                    id: format!("{}:{}-{}", kind, i + 1, end_line),
                    kind: kind.to_string(),
                    name,
                    signature: line.trim().to_string(),
                    start_line: i + 1,
                    end_line,
                });
                break;
            }
        }
    }
    items
}

fn parse_python(content: &str) -> Vec<SkeletonItem> {
    let lines: Vec<&str> = content.lines().collect();
    let mut items = Vec::new();

    let fn_re = Regex::new(r"^(\s*)(async\s+)?def\s+(\w+)\s*\(([^)]*)\)").unwrap();
    let class_re = Regex::new(r"^(\s*)class\s+(\w+)").unwrap();

    for (i, line) in lines.iter().enumerate() {
        if let Some(cap) = fn_re.captures(line) {
            let name = cap.get(3).map(|m| m.as_str()).unwrap_or("").to_string();
            let end_line = find_python_block_end(&lines, i);
            items.push(SkeletonItem {
                id: format!("function:{}-{}", i + 1, end_line),
                kind: "function".to_string(),
                name,
                signature: line.trim().to_string(),
                start_line: i + 1,
                end_line,
            });
        } else if let Some(cap) = class_re.captures(line) {
            let name = cap.get(2).map(|m| m.as_str()).unwrap_or("").to_string();
            let end_line = find_python_block_end(&lines, i);
            items.push(SkeletonItem {
                id: format!("class:{}-{}", i + 1, end_line),
                kind: "class".to_string(),
                name,
                signature: line.trim().to_string(),
                start_line: i + 1,
                end_line,
            });
        }
    }
    items
}

fn parse_js_ts(content: &str) -> Vec<SkeletonItem> {
    let lines: Vec<&str> = content.lines().collect();
    let mut items = Vec::new();

    let patterns = [
        (Regex::new(r"^(\s*)(export\s+)?(default\s+)?(async\s+)?function\s+(\w+)").unwrap(), "function"),
        (Regex::new(r"^(\s*)(export\s+)?(abstract\s+)?class\s+(\w+)").unwrap(), "class"),
        (Regex::new(r"^(\s*)(export\s+)?(const|let|var)\s+(\w+)\s*=\s*(async\s*)?\(").unwrap(), "arrow_fn"),
        (Regex::new(r"^(\s*)(export\s+)?interface\s+(\w+)").unwrap(), "interface"),
        (Regex::new(r"^(\s*)(export\s+)?type\s+(\w+)").unwrap(), "type"),
    ];

    for (i, line) in lines.iter().enumerate() {
        for (re, kind) in &patterns {
            if let Some(cap) = re.captures(line) {
                let name = cap.get(cap.len() - 1).map(|m| m.as_str()).unwrap_or("").to_string();
                if name.is_empty() || ["async", "function", "class", "interface", "type"].contains(&name.as_str()) {
                    continue;
                }
                let end_line = find_block_end(&lines, i);
                items.push(SkeletonItem {
                    id: format!("{}:{}-{}", kind, i + 1, end_line),
                    kind: kind.to_string(),
                    name,
                    signature: line.trim().to_string(),
                    start_line: i + 1,
                    end_line,
                });
                break;
            }
        }
    }
    items
}

fn parse_go(content: &str) -> Vec<SkeletonItem> {
    let lines: Vec<&str> = content.lines().collect();
    let mut items = Vec::new();

    let fn_re = Regex::new(r"^func\s+(\([\w\s*]+\)\s+)?(\w+)\s*\(").unwrap();
    let type_re = Regex::new(r"^type\s+(\w+)\s+(struct|interface)").unwrap();

    for (i, line) in lines.iter().enumerate() {
        if let Some(cap) = fn_re.captures(line) {
            let name = cap.get(2).map(|m| m.as_str()).unwrap_or("").to_string();
            let end_line = find_block_end(&lines, i);
            let kind = if cap.get(1).is_some() { "method" } else { "function" };
            items.push(SkeletonItem {
                id: format!("{}:{}-{}", kind, i + 1, end_line),
                kind: kind.to_string(),
                name,
                signature: line.trim().to_string(),
                start_line: i + 1,
                end_line,
            });
        } else if let Some(cap) = type_re.captures(line) {
            let name = cap.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            let kind = cap.get(2).map(|m| m.as_str()).unwrap_or("struct");
            let end_line = find_block_end(&lines, i);
            items.push(SkeletonItem {
                id: format!("{}:{}-{}", kind, i + 1, end_line),
                kind: kind.to_string(),
                name,
                signature: line.trim().to_string(),
                start_line: i + 1,
                end_line,
            });
        }
    }
    items
}

fn parse_generic(content: &str) -> Vec<SkeletonItem> {
    let lines: Vec<&str> = content.lines().collect();
    let mut items = Vec::new();
    let fn_re = Regex::new(r"(?:function|def|fn|func)\s+(\w+)").unwrap();

    for (i, line) in lines.iter().enumerate() {
        if let Some(cap) = fn_re.captures(line) {
            let name = cap.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            let end_line = find_block_end(&lines, i);
            items.push(SkeletonItem {
                id: format!("function:{}-{}", i + 1, end_line),
                kind: "function".to_string(),
                name,
                signature: line.trim().to_string(),
                start_line: i + 1,
                end_line,
            });
        }
    }
    items
}

fn find_block_end(lines: &[&str], start: usize) -> usize {
    let mut depth = 0i32;
    for (i, line) in lines[start..].iter().enumerate() {
        for ch in line.chars() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth <= 0 && i > 0 {
                        return start + i + 1;
                    }
                }
                _ => {}
            }
        }
    }
    lines.len()
}

fn find_python_block_end(lines: &[&str], start: usize) -> usize {
    let base_indent = lines[start].len() - lines[start].trim_start().len();
    for (i, line) in lines[start + 1..].iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent <= base_indent && !line.trim().is_empty() {
            return start + i + 1;
        }
    }
    lines.len()
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadCodeBodyParams {
    #[schemars(description = "Root-relative path to the code file")]
    pub path: String,
    #[schemars(description = "List of skeleton IDs from read_code_skeleton (e.g. 'function:10-25')")]
    pub ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CodeBodyItem {
    pub id: String,
    pub content: String,
}

pub struct ReadCodeBodyResult {
    pub items: Vec<CodeBodyItem>,
    pub token_count: usize,
}

pub fn read_code_body(root: &Path, params: ReadCodeBodyParams) -> anyhow::Result<ReadCodeBodyResult> {
    let path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&path)?;
    let lines: Vec<&str> = content.lines().collect();

    let mut items = Vec::new();
    for id in &params.ids {
        let parts: Vec<&str> = id.rsplitn(2, ':').collect();
        if parts.len() == 2 {
            let range: Vec<&str> = parts[0].splitn(2, '-').collect();
            if range.len() == 2 {
                if let (Ok(start), Ok(end)) = (range[0].parse::<usize>(), range[1].parse::<usize>()) {
                    let start = start.saturating_sub(1);
                    let end = end.min(lines.len());
                    let body = lines[start..end].join("\n");
                    items.push(CodeBodyItem { id: id.clone(), content: body });
                    continue;
                }
            }
        }
        items.push(CodeBodyItem { id: id.clone(), content: format!("Error: invalid id '{}'", id) });
    }

    let json = serde_json::to_string(&items).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadCodeBodyResult { items, token_count })
}
