//! Phase 14 — opt-in mutating write tools (gated behind `--enable-writes`).
//!
//! Completes the symbol CRUD that `patch_symbol` (update) and `rename_symbol`
//! started: `create_file` (new files), `delete_symbol` / `insert_symbol`
//! (delete/create symbols), and `apply_edits` (atomic multi-file find/replace).
//! All share the house rules: `dry_run` preview, stale-line guards, CRLF /
//! trailing-newline preservation, and diff/summary-only output (never the full
//! file body).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::fs::estimate_tokens;
use super::patch::parse_id;
use crate::security::safe_path;

pub(crate) fn unified_diff(old: &str, new: &str) -> String {
    similar::TextDiff::from_lines(old, new)
        .unified_diff()
        .context_radius(2)
        .header("before", "after")
        .to_string()
}

// ── create_file ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateFileParams {
    #[schemars(description = "Root-relative path of the file to create")]
    pub path: String,
    #[schemars(description = "Full file content to write")]
    pub content: String,
    #[schemars(description = "Allow replacing an existing file (default false — refuses to overwrite)")]
    pub overwrite: Option<bool>,
    #[schemars(description = "true = report what would happen without writing (default false)")]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct CreateFileResult {
    pub path: String,
    pub bytes: usize,
    pub created: bool,
    pub overwritten: bool,
    pub written: bool,
}

pub fn create_file(root: &Path, params: CreateFileParams) -> anyhow::Result<CreateFileResult> {
    let path = safe_path(root, &params.path)?;
    let exists = path.exists();
    let overwrite = params.overwrite.unwrap_or(false);
    if exists && !overwrite {
        anyhow::bail!(
            "file already exists: {} — pass overwrite:true to replace it",
            params.path
        );
    }
    let dry_run = params.dry_run.unwrap_or(false);
    if !dry_run {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &params.content)?;
    }
    Ok(CreateFileResult {
        path: params.path,
        bytes: params.content.len(),
        created: !exists,
        overwritten: exists,
        written: !dry_run,
    })
}

// ── delete_symbol ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteSymbolParams {
    #[schemars(description = "Root-relative path to the code file")]
    pub path: String,
    #[schemars(description = "Skeleton ID from read_code_skeleton (e.g. 'function:10-25'). Re-run read_code_skeleton after any other edit to the same file.")]
    pub id: String,
    #[schemars(description = "Symbol name expected inside the deleted range. Strongly recommended: deletion is rejected if not found there, catching stale line numbers.")]
    pub expected_name: Option<String>,
    #[schemars(description = "true = return the would-be diff without writing (default false)")]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct DeleteSymbolResult {
    pub removed_lines: usize,
    pub diff: String,
    pub written: bool,
}

pub fn delete_symbol(root: &Path, params: DeleteSymbolParams) -> anyhow::Result<DeleteSymbolResult> {
    let path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&path)?;

    let (_, start, end) = parse_id(&params.id)
        .ok_or_else(|| anyhow::anyhow!("invalid id '{}' — expected 'kind:start-end'", params.id))?;

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
        && !old_range.iter().any(|l| l.contains(name.as_str()))
    {
        anyhow::bail!(
            "expected_name '{}' not found in lines {}-{} — skeleton is stale, re-run read_code_skeleton",
            name, start, end
        );
    }

    // Drop the symbol's lines, plus one trailing blank line so no gap is left.
    let mut cut_end = end; // 1-based inclusive
    if cut_end < lines.len() && lines[cut_end].trim().is_empty() {
        cut_end += 1;
    }
    let removed_lines = cut_end - (start - 1);

    let mut new_lines: Vec<&str> = Vec::with_capacity(lines.len());
    new_lines.extend_from_slice(&lines[..start - 1]);
    new_lines.extend_from_slice(&lines[cut_end..]);

    let eol = if uses_crlf { "\r\n" } else { "\n" };
    let mut new_content = new_lines.join(eol);
    if had_trailing_newline && !new_content.is_empty() {
        new_content.push_str(eol);
    }

    let diff = unified_diff(&old_range.join("\n"), "");

    let dry_run = params.dry_run.unwrap_or(false);
    if !dry_run {
        std::fs::write(&path, &new_content)?;
    }
    Ok(DeleteSymbolResult {
        removed_lines,
        diff,
        written: !dry_run,
    })
}

// ── insert_symbol ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct InsertSymbolParams {
    #[schemars(description = "Root-relative path to the code file")]
    pub path: String,
    #[schemars(description = "Code to insert (a function, method, import, etc.)")]
    pub content: String,
    #[schemars(description = "Where to insert: 'after_symbol' / 'before_symbol' (need anchor_id), 'after_imports', or 'end_of_file'")]
    pub mode: String,
    #[schemars(description = "Skeleton ID anchor for after_symbol / before_symbol modes")]
    pub anchor_id: Option<String>,
    #[schemars(description = "true = return the would-be diff without writing (default false)")]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct InsertSymbolResult {
    pub inserted_at_line: usize,
    pub diff: String,
    pub written: bool,
}

static IMPORT_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(use\s|import\s|from\s+\S+\s+import|#include|require\s*\(|using\s|package\s)")
        .unwrap()
});

/// Number of leading lines to keep before the last import-like line (0 if none).
fn import_boundary(lines: &[&str]) -> usize {
    let mut last = 0usize;
    for (i, line) in lines.iter().enumerate() {
        if IMPORT_LINE.is_match(line) {
            last = i + 1; // insert after this line
        }
        // Stop scanning once we are clearly past the header region.
        if i > 0 && last > 0 && i - (last.saturating_sub(1)) > 5 {
            break;
        }
    }
    last
}

pub fn insert_symbol(root: &Path, params: InsertSymbolParams) -> anyhow::Result<InsertSymbolResult> {
    let path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&path)?;

    let uses_crlf = content.contains("\r\n");
    let had_trailing_newline = content.ends_with('\n');
    let lines: Vec<&str> = content.lines().collect();

    let insert_at: usize = match params.mode.as_str() {
        "after_symbol" => {
            let id = params
                .anchor_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("mode 'after_symbol' requires anchor_id"))?;
            let (_, _, end) = parse_id(id)
                .ok_or_else(|| anyhow::anyhow!("invalid anchor_id '{id}'"))?;
            end.min(lines.len())
        }
        "before_symbol" => {
            let id = params
                .anchor_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("mode 'before_symbol' requires anchor_id"))?;
            let (_, start, _) = parse_id(id)
                .ok_or_else(|| anyhow::anyhow!("invalid anchor_id '{id}'"))?;
            (start - 1).min(lines.len())
        }
        "after_imports" => import_boundary(&lines),
        "end_of_file" => lines.len(),
        other => anyhow::bail!(
            "unknown mode '{other}' — use after_symbol / before_symbol / after_imports / end_of_file"
        ),
    };

    let content_lines: Vec<&str> = params.content.lines().collect();
    let mut new_lines: Vec<&str> = Vec::with_capacity(lines.len() + content_lines.len() + 2);
    new_lines.extend_from_slice(&lines[..insert_at]);
    // Blank line before, unless at the very top or already preceded by blank.
    if insert_at > 0 && !lines[insert_at - 1].trim().is_empty() {
        new_lines.push("");
    }
    new_lines.extend_from_slice(&content_lines);
    // Blank line after, unless at EOF or already followed by blank.
    if insert_at < lines.len() && !lines[insert_at].trim().is_empty() {
        new_lines.push("");
    }
    new_lines.extend_from_slice(&lines[insert_at..]);

    let eol = if uses_crlf { "\r\n" } else { "\n" };
    let mut new_content = new_lines.join(eol);
    if had_trailing_newline && !new_content.ends_with(eol) {
        new_content.push_str(eol);
    }

    let diff = unified_diff("", &params.content);

    let dry_run = params.dry_run.unwrap_or(false);
    if !dry_run {
        std::fs::write(&path, &new_content)?;
    }
    Ok(InsertSymbolResult {
        inserted_at_line: insert_at + 1,
        diff,
        written: !dry_run,
    })
}

// ── apply_edits (atomic multi-file find/replace) ───────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FileEdit {
    #[schemars(description = "Root-relative path to the file to edit")]
    pub path: String,
    #[schemars(description = "Exact text to find. Must match exactly once in the file (as edits so far leave it).")]
    pub find: String,
    #[schemars(description = "Replacement text")]
    pub replace: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ApplyEditsParams {
    #[schemars(description = "Edits to apply across one or more files, in order. Applied atomically: if any edit fails, nothing is written.")]
    pub edits: Vec<FileEdit>,
    #[schemars(description = "true = validate and report the changes without writing (default false)")]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct EditChange {
    pub path: String,
    pub line: usize,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Serialize)]
pub struct ApplyEditsResult {
    pub files_changed: usize,
    pub edits_applied: usize,
    pub changes: Vec<EditChange>,
    pub written: bool,
    pub token_count: usize,
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").trim().to_string()
}

pub fn apply_edits(root: &Path, params: ApplyEditsParams) -> anyhow::Result<ApplyEditsResult> {
    if params.edits.is_empty() {
        anyhow::bail!("'edits' must contain at least one edit");
    }

    // In-memory working copy per file; nothing is written until all edits validate.
    let mut working: HashMap<String, (std::path::PathBuf, String)> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut changed: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut changes: Vec<EditChange> = Vec::new();

    for (i, edit) in params.edits.iter().enumerate() {
        if edit.find.is_empty() {
            anyhow::bail!("edits[{i}]: 'find' must not be empty");
        }
        if !working.contains_key(&edit.path) {
            let abs = safe_path(root, &edit.path)?;
            let content = std::fs::read_to_string(&abs)
                .map_err(|e| anyhow::anyhow!("edits[{i}]: cannot read {}: {e}", edit.path))?;
            working.insert(edit.path.clone(), (abs, content));
            order.push(edit.path.clone());
        }
        let (_, content) = working.get_mut(&edit.path).unwrap();

        let match_lines: Vec<usize> = content
            .match_indices(&edit.find)
            .map(|(offset, _)| content[..offset].matches('\n').count() + 1)
            .collect();
        match match_lines.len() {
            0 => anyhow::bail!(
                "edits[{i}]: 'find' not found in {} — nothing written (atomic)",
                edit.path
            ),
            1 => {
                let line = match_lines[0];
                *content = content.replacen(&edit.find, &edit.replace, 1);
                changed.insert(edit.path.clone());
                changes.push(EditChange {
                    path: edit.path.clone(),
                    line,
                    before: first_line(&edit.find),
                    after: first_line(&edit.replace),
                });
            }
            n => anyhow::bail!(
                "edits[{i}]: 'find' matches {n} times in {} (lines {match_lines:?}) — add surrounding context so it is unique. Nothing written (atomic).",
                edit.path
            ),
        }
    }

    let dry_run = params.dry_run.unwrap_or(false);
    if !dry_run {
        for path_key in &order {
            if changed.contains(path_key) {
                let (abs, content) = &working[path_key];
                std::fs::write(abs, content)?;
            }
        }
    }

    let json = serde_json::to_string(&changes).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ApplyEditsResult {
        files_changed: changed.len(),
        edits_applied: changes.len(),
        changes,
        written: !dry_run,
        token_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, content) in files {
            let p = dir.path().join(name);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(p, content).unwrap();
        }
        dir
    }

    #[test]
    fn create_file_refuses_overwrite_by_default() {
        let dir = setup(&[("a.txt", "old")]);
        let err = create_file(
            dir.path(),
            CreateFileParams { path: "a.txt".into(), content: "new".into(), overwrite: None, dry_run: None },
        );
        assert!(err.is_err());
        assert_eq!(std::fs::read_to_string(dir.path().join("a.txt")).unwrap(), "old");
    }

    #[test]
    fn create_file_writes_new_and_makes_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let r = create_file(
            dir.path(),
            CreateFileParams { path: "src/new/mod.rs".into(), content: "fn x() {}\n".into(), overwrite: None, dry_run: None },
        )
        .unwrap();
        assert!(r.created && r.written);
        assert_eq!(std::fs::read_to_string(dir.path().join("src/new/mod.rs")).unwrap(), "fn x() {}\n");
    }

    #[test]
    fn create_file_dry_run_does_not_write() {
        let dir = tempfile::tempdir().unwrap();
        let r = create_file(
            dir.path(),
            CreateFileParams { path: "a.txt".into(), content: "hi".into(), overwrite: None, dry_run: Some(true) },
        )
        .unwrap();
        assert!(!r.written);
        assert!(!dir.path().join("a.txt").exists());
    }

    #[test]
    fn delete_symbol_removes_range_and_trailing_blank() {
        let dir = setup(&[("a.rs", "fn keep() {}\n\nfn drop_me() {\n    x();\n}\n\nfn after() {}\n")]);
        let r = delete_symbol(
            dir.path(),
            DeleteSymbolParams { path: "a.rs".into(), id: "function:3-5".into(), expected_name: Some("drop_me".into()), dry_run: None },
        )
        .unwrap();
        assert!(r.written);
        let out = std::fs::read_to_string(dir.path().join("a.rs")).unwrap();
        assert!(!out.contains("drop_me"));
        assert!(out.contains("fn keep()"));
        assert!(out.contains("fn after()"));
    }

    #[test]
    fn delete_symbol_stale_name_rejected() {
        let dir = setup(&[("a.rs", "fn a() {}\n")]);
        let r = delete_symbol(
            dir.path(),
            DeleteSymbolParams { path: "a.rs".into(), id: "function:1-1".into(), expected_name: Some("nonexistent".into()), dry_run: None },
        );
        assert!(r.is_err());
    }

    #[test]
    fn insert_symbol_after_symbol() {
        let dir = setup(&[("a.rs", "fn first() {}\n")]);
        let r = insert_symbol(
            dir.path(),
            InsertSymbolParams {
                path: "a.rs".into(),
                content: "fn second() {}".into(),
                mode: "after_symbol".into(),
                anchor_id: Some("function:1-1".into()),
                dry_run: None,
            },
        )
        .unwrap();
        assert!(r.written);
        let out = std::fs::read_to_string(dir.path().join("a.rs")).unwrap();
        assert!(out.contains("fn first()"));
        assert!(out.contains("fn second()"));
        assert!(out.find("first").unwrap() < out.find("second").unwrap());
    }

    #[test]
    fn insert_symbol_after_imports() {
        let dir = setup(&[("a.rs", "use std::io;\nuse std::fmt;\n\nfn main() {}\n")]);
        insert_symbol(
            dir.path(),
            InsertSymbolParams {
                path: "a.rs".into(),
                content: "use std::path::Path;".into(),
                mode: "after_imports".into(),
                anchor_id: None,
                dry_run: None,
            },
        )
        .unwrap();
        let out = std::fs::read_to_string(dir.path().join("a.rs")).unwrap();
        let path_pos = out.find("use std::path::Path;").unwrap();
        let main_pos = out.find("fn main()").unwrap();
        assert!(path_pos < main_pos, "import should land before main");
    }

    #[test]
    fn apply_edits_atomic_multifile() {
        let dir = setup(&[("a.rs", "let x = OLD;\n"), ("b.rs", "let y = OLD;\n")]);
        let r = apply_edits(
            dir.path(),
            ApplyEditsParams {
                edits: vec![
                    FileEdit { path: "a.rs".into(), find: "OLD".into(), replace: "NEW".into() },
                    FileEdit { path: "b.rs".into(), find: "OLD".into(), replace: "NEW".into() },
                ],
                dry_run: None,
            },
        )
        .unwrap();
        assert_eq!(r.files_changed, 2);
        assert_eq!(r.edits_applied, 2);
        assert!(std::fs::read_to_string(dir.path().join("a.rs")).unwrap().contains("NEW"));
        assert!(std::fs::read_to_string(dir.path().join("b.rs")).unwrap().contains("NEW"));
    }

    #[test]
    fn apply_edits_rolls_back_on_any_failure() {
        let dir = setup(&[("a.rs", "let x = OLD;\n"), ("b.rs", "no match here\n")]);
        let r = apply_edits(
            dir.path(),
            ApplyEditsParams {
                edits: vec![
                    FileEdit { path: "a.rs".into(), find: "OLD".into(), replace: "NEW".into() },
                    FileEdit { path: "b.rs".into(), find: "MISSING".into(), replace: "X".into() },
                ],
                dry_run: None,
            },
        );
        assert!(r.is_err());
        // a.rs must be untouched because b.rs's edit failed (atomic).
        assert_eq!(std::fs::read_to_string(dir.path().join("a.rs")).unwrap(), "let x = OLD;\n");
    }

    #[test]
    fn apply_edits_ambiguous_find_rejected() {
        let dir = setup(&[("a.rs", "x x\n")]);
        let r = apply_edits(
            dir.path(),
            ApplyEditsParams {
                edits: vec![FileEdit { path: "a.rs".into(), find: "x".into(), replace: "y".into() }],
                dry_run: None,
            },
        );
        assert!(r.is_err());
    }
}
