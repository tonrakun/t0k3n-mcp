//! patch_symbol — symbol-level writes.
//!
//! Replaces the line range of a skeleton ID (from read_code_skeleton) with
//! new text, so the skeleton → read one body → write one body flow works
//! without ever loading the whole file into the LLM context.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::security::safe_path;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PatchSymbolParams {
    #[schemars(description = "Root-relative path to the code file")]
    pub path: String,
    #[schemars(description = "Skeleton ID from read_code_skeleton (e.g. 'function:10-25'). Line numbers must be from a CURRENT skeleton — re-run read_code_skeleton after any other edit to the same file.")]
    pub id: String,
    #[schemars(description = "Replacement text for the entire line range of the symbol (signature + body, no trailing newline needed)")]
    pub new_body: String,
    #[schemars(description = "Symbol name expected inside the replaced range. Strongly recommended: the patch is rejected if the name is not found there, which catches stale line numbers.")]
    pub expected_name: Option<String>,
    #[schemars(description = "true = return the would-be diff without writing the file (default: false)")]
    pub dry_run: Option<bool>,
}

#[derive(Debug)]
pub struct PatchSymbolResult {
    pub diff: String,
    pub new_id: String,
    pub lines_before: usize,
    pub lines_after: usize,
    pub written: bool,
}

fn parse_id(id: &str) -> Option<(String, usize, usize)> {
    let (kind, range) = id.rsplit_once(':')?;
    let (start, end) = range.split_once('-')?;
    let start: usize = start.parse().ok()?;
    let end: usize = end.parse().ok()?;
    if start == 0 || end < start {
        return None;
    }
    Some((kind.to_string(), start, end))
}

pub fn patch_symbol(root: &Path, params: PatchSymbolParams) -> anyhow::Result<PatchSymbolResult> {
    let path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&path)?;

    let (kind, start, end) = parse_id(&params.id)
        .ok_or_else(|| anyhow::anyhow!("invalid id '{}' — expected format 'kind:start-end'", params.id))?;

    let uses_crlf = content.contains("\r\n");
    let had_trailing_newline = content.ends_with('\n');
    let lines: Vec<&str> = content.lines().collect();

    if end > lines.len() {
        anyhow::bail!(
            "id range {}-{} exceeds file length {} — skeleton is stale, re-run read_code_skeleton",
            start, end, lines.len()
        );
    }

    let old_range = &lines[start - 1..end];
    if let Some(name) = &params.expected_name
        && !old_range.iter().any(|l| l.contains(name.as_str())) {
            anyhow::bail!(
                "expected_name '{}' not found in lines {}-{} — skeleton is stale, re-run read_code_skeleton",
                name, start, end
            );
        }

    let new_body_lines: Vec<&str> = params.new_body.lines().collect();
    let mut new_lines: Vec<&str> = Vec::with_capacity(lines.len());
    new_lines.extend_from_slice(&lines[..start - 1]);
    new_lines.extend_from_slice(&new_body_lines);
    new_lines.extend_from_slice(&lines[end..]);

    let eol = if uses_crlf { "\r\n" } else { "\n" };
    let mut new_content = new_lines.join(eol);
    if had_trailing_newline {
        new_content.push_str(eol);
    }

    let diff = similar::TextDiff::from_lines(old_range.join("\n"), &params.new_body)
        .unified_diff()
        .context_radius(2)
        .header("before", "after")
        .to_string();

    let dry_run = params.dry_run.unwrap_or(false);
    if !dry_run {
        std::fs::write(&path, &new_content)?;
    }

    let new_end = start + new_body_lines.len().max(1) - 1;
    Ok(PatchSymbolResult {
        diff,
        new_id: format!("{kind}:{start}-{new_end}"),
        lines_before: end - start + 1,
        lines_after: new_body_lines.len(),
        written: !dry_run,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(content: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.rs");
        std::fs::write(&file, content).unwrap();
        (dir, file)
    }

    #[test]
    fn replaces_symbol_range() {
        let (dir, file) = setup("fn a() {}\nfn b() {\n    1\n}\nfn c() {}\n");
        let r = patch_symbol(dir.path(), PatchSymbolParams {
            path: "a.rs".into(),
            id: "function:2-4".into(),
            new_body: "fn b() {\n    2\n}".into(),
            expected_name: Some("b".into()),
            dry_run: None,
        }).unwrap();
        assert!(r.written);
        assert_eq!(r.new_id, "function:2-4");
        let after = std::fs::read_to_string(&file).unwrap();
        assert_eq!(after, "fn a() {}\nfn b() {\n    2\n}\nfn c() {}\n");
    }

    #[test]
    fn stale_name_check_rejects() {
        let (dir, _file) = setup("fn a() {}\nfn b() {}\n");
        let err = patch_symbol(dir.path(), PatchSymbolParams {
            path: "a.rs".into(),
            id: "function:1-1".into(),
            new_body: "fn x() {}".into(),
            expected_name: Some("does_not_exist".into()),
            dry_run: None,
        }).unwrap_err();
        assert!(err.to_string().contains("stale"));
    }

    #[test]
    fn dry_run_does_not_write() {
        let (dir, file) = setup("fn a() {}\n");
        let before = std::fs::read_to_string(&file).unwrap();
        let r = patch_symbol(dir.path(), PatchSymbolParams {
            path: "a.rs".into(),
            id: "function:1-1".into(),
            new_body: "fn a() { todo!() }".into(),
            expected_name: Some("a".into()),
            dry_run: Some(true),
        }).unwrap();
        assert!(!r.written);
        assert!(r.diff.contains("todo!"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
    }

    #[test]
    fn out_of_range_id_rejected() {
        let (dir, _f) = setup("fn a() {}\n");
        let err = patch_symbol(dir.path(), PatchSymbolParams {
            path: "a.rs".into(),
            id: "function:5-9".into(),
            new_body: "x".into(),
            expected_name: None,
            dry_run: None,
        }).unwrap_err();
        assert!(err.to_string().contains("stale"));
    }

    #[test]
    fn preserves_crlf() {
        let (dir, file) = setup("fn a() {}\r\nfn b() {}\r\n");
        patch_symbol(dir.path(), PatchSymbolParams {
            path: "a.rs".into(),
            id: "function:1-1".into(),
            new_body: "fn a() { 1; }".into(),
            expected_name: Some("a".into()),
            dry_run: None,
        }).unwrap();
        let after = std::fs::read_to_string(&file).unwrap();
        assert_eq!(after, "fn a() { 1; }\r\nfn b() {}\r\n");
    }
}
