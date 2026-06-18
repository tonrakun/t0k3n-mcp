//! Phase 18 — write_markdown_section: write counterpart of read_markdown_toc /
//! read_markdown_section. Mirrors patch_symbol / insert_symbol / delete_symbol's
//! CRUD split (dry_run preview, stale-anchor guard, CRLF / trailing-newline
//! preservation, diff-only output), anchored on Markdown headings instead of
//! skeleton IDs.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::security::safe_path;
use super::markdown::scan_headings;
use super::writes::unified_diff;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WriteMarkdownSectionParams {
    #[schemars(description = "Root-relative path to the Markdown file")]
    pub path: String,
    #[schemars(description = "'replace' (swap an existing section's full text, heading included), 'insert_before' / 'insert_after' (add a new block relative to the anchor's section), 'append' (add at the end of the file; anchor not required), or 'delete' (remove the section; content not required)")]
    pub mode: String,
    #[schemars(description = "Heading anchor from read_markdown_toc. Required for replace/insert_before/insert_after/delete; ignored for append")]
    pub anchor: Option<String>,
    #[schemars(description = "Full Markdown text to write, including the heading line(s) for replace/insert modes. Not used for delete")]
    pub content: Option<String>,
    #[schemars(description = "Heading title expected at anchor. Strongly recommended: rejected if it doesn't match, catching a stale TOC")]
    pub expected_title: Option<String>,
    #[schemars(description = "true = return the would-be diff without writing (default false)")]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct WriteMarkdownSectionResult {
    pub diff: String,
    pub written: bool,
}

pub fn write_markdown_section(root: &Path, params: WriteMarkdownSectionParams) -> anyhow::Result<WriteMarkdownSectionResult> {
    let path = safe_path(root, &params.path)?;
    let file_content = std::fs::read_to_string(&path)?;

    let uses_crlf = file_content.contains("\r\n");
    let had_trailing_newline = file_content.ends_with('\n');
    let lines: Vec<&str> = file_content.lines().collect();
    let headings = scan_headings(&lines);

    let resolve_anchor = |anchor: &str, expected_title: &Option<String>| -> anyhow::Result<(usize, usize)> {
        let idx = headings
            .iter()
            .position(|h| h.anchor == anchor)
            .ok_or_else(|| anyhow::anyhow!("anchor '{anchor}' not found — re-run read_markdown_toc"))?;
        let h = &headings[idx];
        if let Some(title) = expected_title
            && &h.title != title
        {
            anyhow::bail!(
                "expected_title '{title}' does not match actual heading '{}' at anchor '{anchor}' — TOC is stale, re-run read_markdown_toc",
                h.title
            );
        }
        let end_line = headings[idx + 1..]
            .iter()
            .find(|n| n.level <= h.level)
            .map(|n| n.line_idx)
            .unwrap_or(lines.len());
        Ok((h.line_idx, end_line))
    };

    let (diff_old, diff_new, new_lines): (String, String, Vec<&str>) = match params.mode.as_str() {
        "replace" => {
            let anchor = params.anchor.as_deref().ok_or_else(|| anyhow::anyhow!("mode 'replace' requires anchor"))?;
            let new_content = params.content.as_deref().ok_or_else(|| anyhow::anyhow!("mode 'replace' requires content"))?;
            let (start, end) = resolve_anchor(anchor, &params.expected_title)?;
            let old_text = lines[start..end].join("\n");

            let mut out: Vec<&str> = Vec::with_capacity(lines.len());
            out.extend_from_slice(&lines[..start]);
            out.extend(new_content.lines());
            out.extend_from_slice(&lines[end..]);
            (old_text, new_content.to_string(), out)
        }
        "insert_before" | "insert_after" => {
            let anchor = params.anchor.as_deref().ok_or_else(|| anyhow::anyhow!("mode '{}' requires anchor", params.mode))?;
            let new_content = params.content.as_deref().ok_or_else(|| anyhow::anyhow!("mode '{}' requires content", params.mode))?;
            let (start, end) = resolve_anchor(anchor, &params.expected_title)?;
            let insert_at = if params.mode == "insert_before" { start } else { end };

            let mut out: Vec<&str> = Vec::with_capacity(lines.len() + new_content.lines().count() + 2);
            out.extend_from_slice(&lines[..insert_at]);
            if insert_at > 0 && !lines[insert_at - 1].trim().is_empty() {
                out.push("");
            }
            out.extend(new_content.lines());
            if insert_at < lines.len() && !lines[insert_at].trim().is_empty() {
                out.push("");
            }
            out.extend_from_slice(&lines[insert_at..]);
            (String::new(), new_content.to_string(), out)
        }
        "append" => {
            let new_content = params.content.as_deref().ok_or_else(|| anyhow::anyhow!("mode 'append' requires content"))?;

            let mut out: Vec<&str> = Vec::with_capacity(lines.len() + new_content.lines().count() + 1);
            out.extend_from_slice(&lines);
            if !out.is_empty() && !out.last().unwrap().trim().is_empty() {
                out.push("");
            }
            out.extend(new_content.lines());
            (String::new(), new_content.to_string(), out)
        }
        "delete" => {
            let anchor = params.anchor.as_deref().ok_or_else(|| anyhow::anyhow!("mode 'delete' requires anchor"))?;
            let (start, end) = resolve_anchor(anchor, &params.expected_title)?;
            let old_text = lines[start..end].join("\n");

            let mut cut_end = end;
            if cut_end < lines.len() && lines[cut_end].trim().is_empty() {
                cut_end += 1;
            }
            let mut out: Vec<&str> = Vec::with_capacity(lines.len());
            out.extend_from_slice(&lines[..start]);
            out.extend_from_slice(&lines[cut_end..]);
            (old_text, String::new(), out)
        }
        other => anyhow::bail!("unknown mode '{other}' — use replace/insert_before/insert_after/append/delete"),
    };

    let eol = if uses_crlf { "\r\n" } else { "\n" };
    let mut new_full = new_lines.join(eol);
    if had_trailing_newline && !new_full.is_empty() && !new_full.ends_with(eol) {
        new_full.push_str(eol);
    }

    let diff = unified_diff(&diff_old, &diff_new);

    let dry_run = params.dry_run.unwrap_or(false);
    if !dry_run {
        std::fs::write(&path, &new_full)?;
    }
    Ok(WriteMarkdownSectionResult { diff, written: !dry_run })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(content: &str) -> (tempfile::TempDir, &'static str) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("doc.md"), content).unwrap();
        (dir, "doc.md")
    }

    const DOC: &str = "# T\n\n## 1. A\n\nbody1\n\n## 2. B\n\nbody2\n";

    #[test]
    fn replace_swaps_section_and_keeps_rest() {
        let (dir, name) = setup(DOC);
        let r = write_markdown_section(
            dir.path(),
            WriteMarkdownSectionParams {
                path: name.into(),
                mode: "replace".into(),
                anchor: Some("1-a".into()),
                content: Some("## 1. A\n\nnew body".into()),
                expected_title: Some("1. A".into()),
                dry_run: None,
            },
        )
        .unwrap();
        assert!(r.written);
        let out = std::fs::read_to_string(dir.path().join(name)).unwrap();
        assert!(out.contains("new body"));
        assert!(!out.contains("body1"));
        assert!(out.contains("body2"));
    }

    #[test]
    fn replace_rejects_stale_expected_title() {
        let (dir, name) = setup(DOC);
        let r = write_markdown_section(
            dir.path(),
            WriteMarkdownSectionParams {
                path: name.into(),
                mode: "replace".into(),
                anchor: Some("1-a".into()),
                content: Some("## 1. A\n\nnew body".into()),
                expected_title: Some("Wrong Title".into()),
                dry_run: None,
            },
        );
        assert!(r.is_err());
    }

    #[test]
    fn delete_removes_section_only() {
        let (dir, name) = setup(DOC);
        write_markdown_section(
            dir.path(),
            WriteMarkdownSectionParams {
                path: name.into(),
                mode: "delete".into(),
                anchor: Some("1-a".into()),
                content: None,
                expected_title: None,
                dry_run: None,
            },
        )
        .unwrap();
        let out = std::fs::read_to_string(dir.path().join(name)).unwrap();
        assert!(!out.contains("## 1. A"));
        assert!(!out.contains("body1"));
        assert!(out.contains("## 2. B"));
        assert!(out.contains("body2"));
    }

    #[test]
    fn insert_after_lands_between_sections() {
        let (dir, name) = setup(DOC);
        write_markdown_section(
            dir.path(),
            WriteMarkdownSectionParams {
                path: name.into(),
                mode: "insert_after".into(),
                anchor: Some("1-a".into()),
                content: Some("## 1.5 New\n\ninserted".into()),
                expected_title: None,
                dry_run: None,
            },
        )
        .unwrap();
        let out = std::fs::read_to_string(dir.path().join(name)).unwrap();
        let pos_a = out.find("## 1. A").unwrap();
        let pos_new = out.find("## 1.5 New").unwrap();
        let pos_b = out.find("## 2. B").unwrap();
        assert!(pos_a < pos_new && pos_new < pos_b);
    }

    #[test]
    fn append_adds_to_end_of_file() {
        let (dir, name) = setup(DOC);
        write_markdown_section(
            dir.path(),
            WriteMarkdownSectionParams {
                path: name.into(),
                mode: "append".into(),
                anchor: None,
                content: Some("## 3. C\n\nbody3".into()),
                expected_title: None,
                dry_run: None,
            },
        )
        .unwrap();
        let out = std::fs::read_to_string(dir.path().join(name)).unwrap();
        assert!(out.trim_end().ends_with("body3"));
    }

    #[test]
    fn dry_run_does_not_write() {
        let (dir, name) = setup(DOC);
        let r = write_markdown_section(
            dir.path(),
            WriteMarkdownSectionParams {
                path: name.into(),
                mode: "delete".into(),
                anchor: Some("1-a".into()),
                content: None,
                expected_title: None,
                dry_run: Some(true),
            },
        )
        .unwrap();
        assert!(!r.written);
        let out = std::fs::read_to_string(dir.path().join(name)).unwrap();
        assert!(out.contains("body1"));
    }

    #[test]
    fn unknown_anchor_rejected() {
        let (dir, name) = setup(DOC);
        let r = write_markdown_section(
            dir.path(),
            WriteMarkdownSectionParams {
                path: name.into(),
                mode: "delete".into(),
                anchor: Some("nope".into()),
                content: None,
                expected_title: None,
                dry_run: None,
            },
        );
        assert!(r.is_err());
    }

    #[test]
    fn preserves_crlf_and_trailing_newline() {
        let (dir, name) = setup("# T\r\n\r\n## 1. A\r\n\r\nbody1\r\n");
        write_markdown_section(
            dir.path(),
            WriteMarkdownSectionParams {
                path: name.into(),
                mode: "replace".into(),
                anchor: Some("1-a".into()),
                content: Some("## 1. A\r\n\r\nnew".into()),
                expected_title: None,
                dry_run: None,
            },
        )
        .unwrap();
        let out = std::fs::read_to_string(dir.path().join(name)).unwrap();
        assert!(out.contains("\r\n"));
        assert!(out.ends_with('\n'));
    }
}
