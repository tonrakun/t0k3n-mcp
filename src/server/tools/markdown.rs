use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::security::safe_path_or_absolute;
use super::fs::estimate_tokens;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadMarkdownTocParams {
    #[schemars(description = "Root-relative or absolute path to the Markdown file (absolute allowed for convert_document tmp files)")]
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TocEntry {
    pub level: u8,
    pub title: String,
    pub anchor: String,
}

pub struct ReadMarkdownTocResult {
    pub toc: Vec<TocEntry>,
    pub token_count: usize,
}

pub fn read_markdown_toc(root: &Path, params: ReadMarkdownTocParams) -> anyhow::Result<ReadMarkdownTocResult> {
    let path = safe_path_or_absolute(root, &params.path)?;
    let content = std::fs::read_to_string(&path)?;
    let toc = extract_toc(&content);
    let json = serde_json::to_string(&toc).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadMarkdownTocResult { toc, token_count })
}

pub fn extract_toc(content: &str) -> Vec<TocEntry> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    let parser = Parser::new_ext(content, opts);

    let mut toc = Vec::new();
    let mut current_level: Option<u8> = None;
    let mut current_text = String::new();

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                current_level = Some(heading_level_to_u8(level));
                current_text.clear();
            }
            Event::Text(t)
                if current_level.is_some() => {
                    current_text.push_str(&t);
                }
            Event::Code(t)
                if current_level.is_some() => {
                    current_text.push_str(&t);
                }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(level) = current_level.take() {
                    let title = current_text.trim().to_string();
                    let anchor = make_anchor(&title);
                    toc.push(TocEntry { level, title, anchor });
                }
                current_text.clear();
            }
            _ => {}
        }
    }
    toc
}

fn heading_level_to_u8(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

pub fn make_anchor(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else if c == ' ' { '-' } else { '\0' })
        .filter(|&c| c != '\0')
        .collect()
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadMarkdownSectionParams {
    #[schemars(description = "Root-relative path to the Markdown file")]
    pub path: String,
    #[schemars(description = "List of heading anchors to extract (from read_markdown_toc)")]
    pub anchors: Vec<String>,
}

pub struct ReadMarkdownSectionResult {
    pub sections: Vec<SectionContent>,
    pub token_count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SectionContent {
    pub anchor: String,
    pub title: String,
    pub content: String,
}

pub fn read_markdown_section(root: &Path, params: ReadMarkdownSectionParams) -> anyhow::Result<ReadMarkdownSectionResult> {
    let path = safe_path_or_absolute(root, &params.path)?;
    let content = std::fs::read_to_string(&path)?;
    let sections = extract_sections(&content, &params.anchors);
    let json = serde_json::to_string(&sections).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadMarkdownSectionResult { sections, token_count })
}

struct HeadingLine {
    line_idx: usize,
    level: usize,
    title: String,
    anchor: String,
}

/// Scan raw lines for ATX headings, skipping fenced code blocks.
/// Anchors are computed with the same `make_anchor` used by `extract_toc`,
/// so lookups by anchor match regardless of inline formatting (backticks etc.).
fn scan_headings(lines: &[&str]) -> Vec<HeadingLine> {
    let mut in_fence = false;
    let mut fence_marker = "```";
    let mut out = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if !in_fence && (trimmed.starts_with("```") || trimmed.starts_with("~~~")) {
            in_fence = true;
            fence_marker = if trimmed.starts_with("```") { "```" } else { "~~~" };
            continue;
        }
        if in_fence {
            if trimmed.starts_with(fence_marker) {
                in_fence = false;
            }
            continue;
        }
        if !line.starts_with('#') {
            continue;
        }
        let level = line.chars().take_while(|&c| c == '#').count();
        if level > 6 {
            continue;
        }
        let rest = &line[level..];
        if !rest.is_empty() && !rest.starts_with(' ') && !rest.starts_with('\t') {
            continue; // not a heading (e.g. "#hashtag")
        }
        let mut title = rest.trim();
        // strip optional ATX closing sequence ("## Title ##")
        let without_closing = title.trim_end_matches('#');
        if without_closing.len() != title.len() && (without_closing.is_empty() || without_closing.ends_with(' ')) {
            title = without_closing.trim_end();
        }
        let title = title.replace('`', "").replace("**", "");
        let anchor = make_anchor(&title);
        out.push(HeadingLine { line_idx: i, level, title, anchor });
    }
    out
}

pub fn extract_sections(content: &str, anchors: &[String]) -> Vec<SectionContent> {
    let lines: Vec<&str> = content.lines().collect();
    let headings = scan_headings(&lines);

    let mut results = Vec::new();

    for anchor in anchors {
        let Some(idx) = headings.iter().position(|h| &h.anchor == anchor) else {
            continue;
        };
        let h = &headings[idx];
        let end_line = headings[idx + 1..]
            .iter()
            .find(|n| n.level <= h.level)
            .map(|n| n.line_idx)
            .unwrap_or(lines.len());

        results.push(SectionContent {
            anchor: anchor.clone(),
            title: h.title.clone(),
            content: lines[h.line_idx..end_line].join("\n"),
        });
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "# T\n\n## 1. A\n\n### 1.1 First\n\nbody1\n\n### 1.2 Second\n\nbody2\n\n## 2. B\n\n### 2.1 `with_code`（拡張）\n\nbody3\n\n```sh\n# not a heading\n```\n\n### 2.2 Last\n\nbody4\n";

    #[test]
    fn section_stops_at_next_same_level_heading() {
        let sections = extract_sections(DOC, &["11-first".to_string()]);
        assert_eq!(sections.len(), 1);
        assert!(sections[0].content.contains("body1"));
        assert!(!sections[0].content.contains("body2"));
        assert!(!sections[0].content.contains("## 2. B"));
    }

    #[test]
    fn section_with_inline_code_heading_matches() {
        let toc = extract_toc(DOC);
        let anchor = toc.iter().find(|e| e.title.contains("with_code")).unwrap().anchor.clone();
        let sections = extract_sections(DOC, &[anchor]);
        assert_eq!(sections.len(), 1);
        assert!(sections[0].content.contains("body3"));
        assert!(!sections[0].content.contains("body4"));
    }

    #[test]
    fn hash_inside_code_fence_does_not_terminate_section() {
        let sections = extract_sections(DOC, &["21-withcode拡張".to_string()]);
        assert_eq!(sections.len(), 1);
        assert!(sections[0].content.contains("not a heading"));
    }

    #[test]
    fn higher_level_heading_terminates_section() {
        let sections = extract_sections(DOC, &["12-second".to_string()]);
        assert_eq!(sections.len(), 1);
        assert!(sections[0].content.contains("body2"));
        assert!(!sections[0].content.contains("## 2. B"));
    }
}
