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
pub struct SymbolEdit {
    #[schemars(description = "Exact text to find within the symbol's current source. Must match exactly once at the time this edit is applied.")]
    pub find: String,
    #[schemars(description = "Replacement text for the matched occurrence")]
    pub replace: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PatchSymbolParams {
    #[schemars(description = "Root-relative path to the code file")]
    pub path: String,
    #[schemars(description = "Skeleton ID from read_code_skeleton (e.g. 'function:10-25'). Line numbers must be from a CURRENT skeleton — re-run read_code_skeleton after any other edit to the same file.")]
    pub id: String,
    #[schemars(description = "Replacement text for the entire line range of the symbol (signature + body, no trailing newline needed). Provide exactly one of new_body or edits.")]
    pub new_body: Option<String>,
    #[schemars(description = "Partial edits applied inside the symbol's current source, in order. Each find only needs to be unique within the symbol, not the file. Prefer this over new_body for small changes — no need to resend unchanged lines.")]
    pub edits: Option<Vec<SymbolEdit>>,
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

/// Apply find/replace edits sequentially to the symbol's source. Each find
/// must match exactly once in the text as it stands when that edit runs.
/// `symbol_start` is the file line of the symbol's first line, used to report
/// file-absolute line numbers on ambiguous matches.
fn apply_edits(old_text: &str, edits: &[SymbolEdit], symbol_start: usize) -> anyhow::Result<String> {
    if edits.is_empty() {
        anyhow::bail!("'edits' must contain at least one edit");
    }
    let mut text = old_text.to_string();
    for (i, edit) in edits.iter().enumerate() {
        if edit.find.is_empty() {
            anyhow::bail!("edits[{i}]: 'find' must not be empty");
        }
        let match_lines: Vec<usize> = text
            .match_indices(&edit.find)
            .map(|(offset, _)| symbol_start + text[..offset].matches('\n').count())
            .collect();
        match match_lines.len() {
            0 => anyhow::bail!(
                "edits[{i}]: 'find' text not found in the symbol — the symbol source may have changed, re-read it with read_code_body"
            ),
            1 => text = text.replacen(&edit.find, &edit.replace, 1),
            n => anyhow::bail!(
                "edits[{i}]: 'find' text matches {n} times (lines {match_lines:?}) — extend it with surrounding context so it is unique within the symbol"
            ),
        }
    }
    Ok(text)
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

    let old_text = old_range.join("\n");
    let new_body = match (&params.new_body, &params.edits) {
        (Some(body), None) => body.clone(),
        (None, Some(edits)) => apply_edits(&old_text, edits, start)?,
        _ => anyhow::bail!("provide exactly one of 'new_body' or 'edits'"),
    };

    let new_body_lines: Vec<&str> = new_body.lines().collect();
    let mut new_lines: Vec<&str> = Vec::with_capacity(lines.len());
    new_lines.extend_from_slice(&lines[..start - 1]);
    new_lines.extend_from_slice(&new_body_lines);
    new_lines.extend_from_slice(&lines[end..]);

    let eol = if uses_crlf { "\r\n" } else { "\n" };
    let mut new_content = new_lines.join(eol);
    if had_trailing_newline {
        new_content.push_str(eol);
    }

    let diff = similar::TextDiff::from_lines(old_text.as_str(), new_body.as_str())
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
            new_body: Some("fn b() {\n    2\n}".into()),
            edits: None,
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
            new_body: Some("fn x() {}".into()),
            edits: None,
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
            new_body: Some("fn a() { todo!() }".into()),
            edits: None,
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
            new_body: Some("x".into()),
            edits: None,
            expected_name: None,
            dry_run: None,
        }).unwrap_err();
        assert!(err.to_string().contains("stale"));
    }

    #[test]
    fn edits_apply_in_order() {
        let (dir, file) = setup("fn a() {}\nfn b() {\n    let x = 1;\n    x + 1\n}\n");
        let r = patch_symbol(dir.path(), PatchSymbolParams {
            path: "a.rs".into(),
            id: "function:2-5".into(),
            new_body: None,
            edits: Some(vec![
                SymbolEdit { find: "let x = 1;".into(), replace: "let x = 2;".into() },
                SymbolEdit { find: "x + 1".into(), replace: "x * 10".into() },
            ]),
            expected_name: Some("b".into()),
            dry_run: None,
        }).unwrap();
        assert!(r.written);
        assert_eq!(r.new_id, "function:2-5");
        let after = std::fs::read_to_string(&file).unwrap();
        assert_eq!(after, "fn a() {}\nfn b() {\n    let x = 2;\n    x * 10\n}\n");
    }

    #[test]
    fn edit_find_not_found_rejected() {
        let (dir, _f) = setup("fn a() {\n    1\n}\n");
        let err = patch_symbol(dir.path(), PatchSymbolParams {
            path: "a.rs".into(),
            id: "function:1-3".into(),
            new_body: None,
            edits: Some(vec![SymbolEdit { find: "nope".into(), replace: "x".into() }]),
            expected_name: Some("a".into()),
            dry_run: None,
        }).unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn ambiguous_edit_find_reports_lines() {
        let (dir, _f) = setup("fn a() {\n    foo();\n    foo();\n}\n");
        let err = patch_symbol(dir.path(), PatchSymbolParams {
            path: "a.rs".into(),
            id: "function:1-4".into(),
            new_body: None,
            edits: Some(vec![SymbolEdit { find: "foo();".into(), replace: "bar();".into() }]),
            expected_name: Some("a".into()),
            dry_run: None,
        }).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("2 times"));
        assert!(msg.contains("[2, 3]"));
    }

    #[test]
    fn edit_scoped_to_symbol_range_only() {
        // "1" also appears in fn a, but find only needs to be unique inside fn b
        let (dir, file) = setup("fn a() {\n    1\n}\nfn b() {\n    1\n}\n");
        patch_symbol(dir.path(), PatchSymbolParams {
            path: "a.rs".into(),
            id: "function:4-6".into(),
            new_body: None,
            edits: Some(vec![SymbolEdit { find: "    1".into(), replace: "    2".into() }]),
            expected_name: Some("b".into()),
            dry_run: None,
        }).unwrap();
        let after = std::fs::read_to_string(&file).unwrap();
        assert_eq!(after, "fn a() {\n    1\n}\nfn b() {\n    2\n}\n");
    }

    #[test]
    fn edits_can_change_line_count() {
        let (dir, file) = setup("fn a() {\n    1\n}\nfn c() {}\n");
        let r = patch_symbol(dir.path(), PatchSymbolParams {
            path: "a.rs".into(),
            id: "function:1-3".into(),
            new_body: None,
            edits: Some(vec![SymbolEdit { find: "    1".into(), replace: "    let y = 0;\n    y".into() }]),
            expected_name: Some("a".into()),
            dry_run: None,
        }).unwrap();
        assert_eq!(r.new_id, "function:1-4");
        let after = std::fs::read_to_string(&file).unwrap();
        assert_eq!(after, "fn a() {\n    let y = 0;\n    y\n}\nfn c() {}\n");
    }

    #[test]
    fn both_or_neither_body_sources_rejected() {
        let (dir, _f) = setup("fn a() {}\n");
        let both = patch_symbol(dir.path(), PatchSymbolParams {
            path: "a.rs".into(),
            id: "function:1-1".into(),
            new_body: Some("fn a() { 1 }".into()),
            edits: Some(vec![SymbolEdit { find: "{}".into(), replace: "{ 1 }".into() }]),
            expected_name: None,
            dry_run: None,
        }).unwrap_err();
        assert!(both.to_string().contains("exactly one"));
        let neither = patch_symbol(dir.path(), PatchSymbolParams {
            path: "a.rs".into(),
            id: "function:1-1".into(),
            new_body: None,
            edits: None,
            expected_name: None,
            dry_run: None,
        }).unwrap_err();
        assert!(neither.to_string().contains("exactly one"));
    }

    #[test]
    fn edits_preserve_crlf() {
        let (dir, file) = setup("fn a() {\r\n    1\r\n}\r\n");
        patch_symbol(dir.path(), PatchSymbolParams {
            path: "a.rs".into(),
            id: "function:1-3".into(),
            new_body: None,
            edits: Some(vec![SymbolEdit { find: "    1".into(), replace: "    2".into() }]),
            expected_name: Some("a".into()),
            dry_run: None,
        }).unwrap();
        let after = std::fs::read_to_string(&file).unwrap();
        assert_eq!(after, "fn a() {\r\n    2\r\n}\r\n");
    }

    #[test]
    fn preserves_crlf() {
        let (dir, file) = setup("fn a() {}\r\nfn b() {}\r\n");
        patch_symbol(dir.path(), PatchSymbolParams {
            path: "a.rs".into(),
            id: "function:1-1".into(),
            new_body: Some("fn a() { 1; }".into()),
            edits: None,
            expected_name: Some("a".into()),
            dry_run: None,
        }).unwrap();
        let after = std::fs::read_to_string(&file).unwrap();
        assert_eq!(after, "fn a() { 1; }\r\nfn b() {}\r\n");
    }
}
