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
            Event::Text(t) => {
                if current_level.is_some() {
                    current_text.push_str(&t);
                }
            }
            Event::Code(t) => {
                if current_level.is_some() {
                    current_text.push_str(&t);
                }
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

pub fn extract_sections(content: &str, anchors: &[String]) -> Vec<SectionContent> {
    let lines: Vec<&str> = content.lines().collect();
    let toc = extract_toc(content);

    let mut results = Vec::new();

    for anchor in anchors {
        let Some(idx) = toc.iter().position(|e| &e.anchor == anchor) else {
            continue;
        };
        let entry = &toc[idx];
        let next_same_or_higher = toc[idx + 1..]
            .iter()
            .find(|e| e.level <= entry.level)
            .map(|e| e.anchor.clone());

        let section_lines = extract_section_lines(&lines, &entry.title, next_same_or_higher.as_deref());
        results.push(SectionContent {
            anchor: anchor.clone(),
            title: entry.title.clone(),
            content: section_lines.join("\n"),
        });
    }
    results
}

fn extract_section_lines<'a>(lines: &[&'a str], title: &str, until_title: Option<&str>) -> Vec<String> {
    let mut in_section = false;
    let mut result = Vec::new();

    for line in lines {
        let stripped = line.trim_start_matches('#').trim();
        if line.starts_with('#') {
            if stripped.eq_ignore_ascii_case(title) {
                in_section = true;
                result.push(line.to_string());
                continue;
            }
            if in_section {
                if let Some(end) = until_title {
                    if stripped.eq_ignore_ascii_case(end) {
                        break;
                    }
                } else {
                    // stop at same or higher level heading
                    let current_level = line.chars().take_while(|&c| c == '#').count();
                    let start_level = result
                        .first()
                        .map(|l: &String| l.chars().take_while(|&c| c == '#').count())
                        .unwrap_or(6);
                    if current_level <= start_level {
                        break;
                    }
                }
            }
        }
        if in_section {
            result.push(line.to_string());
        }
    }
    result
}
