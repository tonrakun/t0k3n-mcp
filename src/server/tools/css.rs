use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::security::{rel_display, safe_path};
use super::fs::estimate_tokens;

// ─── read_css_skeleton ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadCssSkeletonParams {
    #[schemars(description = "Root-relative path to a .css, .scss, or .less file.")]
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct CssSelectorItem {
    pub id: String,
    pub selector: String,
    pub property_count: usize,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Serialize)]
pub struct ReadCssSkeletonResult {
    pub path: String,
    pub selectors: Vec<CssSelectorItem>,
    pub token_count: usize,
}

pub fn read_css_skeleton(root: &Path, params: ReadCssSkeletonParams) -> anyhow::Result<ReadCssSkeletonResult> {
    let file_path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| anyhow::anyhow!("ファイル読み取り失敗: {e}"))?;
    let rel = rel_display(root, &file_path);

    let selectors = parse_css_skeleton(&content);
    let json = serde_json::to_string(&selectors).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadCssSkeletonResult { path: rel, selectors, token_count })
}

pub fn parse_css_skeleton(content: &str) -> Vec<CssSelectorItem> {
    let lines: Vec<&str> = content.lines().collect();
    let mut items = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Skip empty, comment, and @-rule lines at top level
        if trimmed.is_empty()
            || trimmed.starts_with("//")
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
            || trimmed.starts_with('@')
        {
            i += 1;
            continue;
        }

        // Selector line ends with '{' (might have inline content too)
        if trimmed.contains('{') && !trimmed.starts_with("//") {
            let selector = trimmed
                .split('{')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();

            if selector.is_empty() {
                i += 1;
                continue;
            }

            let start_line = i + 1; // 1-indexed
            let mut depth: i32 = 0;
            let mut j = i;
            let mut property_count = 0;

            while j < lines.len() {
                let l = lines[j].trim();
                let opens = l.chars().filter(|&c| c == '{').count() as i32;
                let closes = l.chars().filter(|&c| c == '}').count() as i32;
                depth += opens - closes;

                // Count property lines at depth 1: contain ':' but aren't selector lines
                if depth == 1
                    && j > i
                    && l.contains(':')
                    && !l.ends_with('{')
                    && !l.starts_with("//")
                    && !l.starts_with("/*")
                    && !l.starts_with('*')
                {
                    property_count += 1;
                }

                if depth <= 0 { break; }
                j += 1;
            }

            let end_line = (j + 1).min(lines.len()); // 1-indexed closing brace line
            let id = format!("selector:{}-{}", start_line, end_line);
            items.push(CssSelectorItem { id, selector, property_count, start_line, end_line });
            i = j + 1;
            continue;
        }

        i += 1;
    }

    items
}

// ─── read_css_body ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadCssBodyParams {
    #[schemars(description = "Root-relative path to the CSS file (from read_css_skeleton result).")]
    pub path: String,
    #[schemars(description = "List of selector IDs from read_css_skeleton (e.g. 'selector:5-12').")]
    pub ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CssBodyItem {
    pub id: String,
    pub selector: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct ReadCssBodyResult {
    pub items: Vec<CssBodyItem>,
    pub token_count: usize,
}

pub fn read_css_body(root: &Path, params: ReadCssBodyParams) -> anyhow::Result<ReadCssBodyResult> {
    let file_path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| anyhow::anyhow!("ファイル読み取り失敗: {e}"))?;
    let lines: Vec<&str> = content.lines().collect();
    let skeleton = parse_css_skeleton(&content);

    let mut items = Vec::new();
    for id in &params.ids {
        // ID format: "selector:START-END" (1-indexed, inclusive)
        let parts: Vec<&str> = id.rsplitn(2, ':').collect();
        if parts.len() == 2 {
            let range: Vec<&str> = parts[0].splitn(2, '-').collect();
            if range.len() == 2 {
                if let (Ok(start), Ok(end)) =
                    (range[0].parse::<usize>(), range[1].parse::<usize>())
                {
                    let from = start.saturating_sub(1); // 0-indexed selector line
                    let to = end.min(lines.len());
                    let body = lines[from..to].join("\n");
                    let selector = skeleton.iter()
                        .find(|s| &s.id == id)
                        .map(|s| s.selector.clone())
                        .unwrap_or_default();
                    items.push(CssBodyItem { id: id.clone(), selector, content: body });
                    continue;
                }
            }
        }
        items.push(CssBodyItem {
            id: id.clone(),
            selector: String::new(),
            content: format!("Error: invalid id '{id}'"),
        });
    }

    let json = serde_json::to_string(&items).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadCssBodyResult { items, token_count })
}
