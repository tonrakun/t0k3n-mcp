//! manage_imports — add/remove import statements (Phase 15, opt-in write).
//!
//! Operates on whole import lines (e.g. "use std::path::Path;" or
//! "import { x } from 'y';"), so it is language-agnostic. Removes by trimmed
//! equality, adds at the import block boundary (reusing insert_symbol's
//! detection), and de-duplicates against existing and other added lines.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

use super::writes::{import_boundary, unified_diff};
use crate::security::safe_path;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ManageImportsParams {
    #[schemars(description = "Root-relative path to the code file")]
    pub path: String,
    #[schemars(description = "Full import lines to add (e.g. \"use std::path::Path;\"). Duplicates of existing imports are skipped.")]
    pub add: Option<Vec<String>>,
    #[schemars(description = "Full import lines to remove (matched by trimmed equality).")]
    pub remove: Option<Vec<String>>,
    #[schemars(description = "true = return the would-be diff without writing (default false)")]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ManageImportsResult {
    pub added: usize,
    pub removed: usize,
    pub skipped: usize,
    pub diff: String,
    pub written: bool,
}

pub fn manage_imports(
    root: &Path,
    params: ManageImportsParams,
) -> anyhow::Result<ManageImportsResult> {
    let path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&path)?;

    let uses_crlf = content.contains("\r\n");
    let had_trailing_newline = content.ends_with('\n');
    let lines: Vec<&str> = content.lines().collect();

    // 1) Removal: drop lines whose trimmed text matches a remove target.
    let remove_set: HashSet<String> = params
        .remove
        .unwrap_or_default()
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let mut removed = 0usize;
    let mut kept: Vec<String> = Vec::with_capacity(lines.len());
    for line in &lines {
        if remove_set.contains(line.trim()) {
            removed += 1;
        } else {
            kept.push((*line).to_string());
        }
    }

    // 2) Additions: skip ones already present (post-removal) or duplicated.
    let existing: HashSet<String> = kept.iter().map(|l| l.trim().to_string()).collect();
    let mut seen_add: HashSet<String> = HashSet::new();
    let mut to_add: Vec<String> = Vec::new();
    let mut skipped = 0usize;
    for a in params.add.unwrap_or_default() {
        let t = a.trim().to_string();
        if t.is_empty() {
            continue;
        }
        if existing.contains(&t) || !seen_add.insert(t.clone()) {
            skipped += 1;
        } else {
            to_add.push(t);
        }
    }

    // 3) Insert additions at the import boundary of the post-removal lines.
    let kept_refs: Vec<&str> = kept.iter().map(|s| s.as_str()).collect();
    let at = import_boundary(&kept_refs);
    let mut new_lines: Vec<String> = Vec::with_capacity(kept.len() + to_add.len());
    new_lines.extend_from_slice(&kept[..at]);
    new_lines.extend(to_add.iter().cloned());
    new_lines.extend_from_slice(&kept[at..]);
    let added = to_add.len();

    let eol = if uses_crlf { "\r\n" } else { "\n" };
    let mut new_content = new_lines.join(eol);
    if had_trailing_newline && !new_content.is_empty() {
        new_content.push_str(eol);
    }

    let diff = unified_diff(&content, &new_content);
    let changed = added > 0 || removed > 0;
    let dry_run = params.dry_run.unwrap_or(false);
    if changed && !dry_run {
        std::fs::write(&path, &new_content)?;
    }

    Ok(ManageImportsResult {
        added,
        removed,
        skipped,
        diff,
        written: changed && !dry_run,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(name: &str, content: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(name), content).unwrap();
        dir
    }

    #[test]
    fn adds_import_after_existing_block() {
        let dir = setup("a.rs", "use std::io;\nuse std::fmt;\n\nfn main() {}\n");
        let r = manage_imports(
            dir.path(),
            ManageImportsParams {
                path: "a.rs".into(),
                add: Some(vec!["use std::path::Path;".into()]),
                remove: None,
                dry_run: None,
            },
        )
        .unwrap();
        assert_eq!(r.added, 1);
        let out = std::fs::read_to_string(dir.path().join("a.rs")).unwrap();
        let path_pos = out.find("use std::path::Path;").unwrap();
        let main_pos = out.find("fn main()").unwrap();
        assert!(path_pos < main_pos);
    }

    #[test]
    fn dedupes_existing_import() {
        let dir = setup("a.rs", "use std::io;\n\nfn main() {}\n");
        let r = manage_imports(
            dir.path(),
            ManageImportsParams {
                path: "a.rs".into(),
                add: Some(vec!["use std::io;".into()]),
                remove: None,
                dry_run: None,
            },
        )
        .unwrap();
        assert_eq!(r.added, 0);
        assert_eq!(r.skipped, 1);
        assert!(!r.written);
    }

    #[test]
    fn removes_import() {
        let dir = setup("a.rs", "use std::io;\nuse std::fmt;\n\nfn main() {}\n");
        let r = manage_imports(
            dir.path(),
            ManageImportsParams {
                path: "a.rs".into(),
                add: None,
                remove: Some(vec!["use std::fmt;".into()]),
                dry_run: None,
            },
        )
        .unwrap();
        assert_eq!(r.removed, 1);
        let out = std::fs::read_to_string(dir.path().join("a.rs")).unwrap();
        assert!(!out.contains("std::fmt"));
        assert!(out.contains("std::io"));
    }

    #[test]
    fn add_and_remove_together() {
        let dir = setup("a.ts", "import { a } from 'x';\nimport { b } from 'y';\n\nconst z = 1;\n");
        let r = manage_imports(
            dir.path(),
            ManageImportsParams {
                path: "a.ts".into(),
                add: Some(vec!["import { c } from 'z';".into()]),
                remove: Some(vec!["import { b } from 'y';".into()]),
                dry_run: None,
            },
        )
        .unwrap();
        assert_eq!(r.added, 1);
        assert_eq!(r.removed, 1);
        let out = std::fs::read_to_string(dir.path().join("a.ts")).unwrap();
        assert!(out.contains("from 'z'"));
        assert!(!out.contains("from 'y'"));
    }

    #[test]
    fn dry_run_does_not_write() {
        let dir = setup("a.rs", "use std::io;\n\nfn main() {}\n");
        let r = manage_imports(
            dir.path(),
            ManageImportsParams {
                path: "a.rs".into(),
                add: Some(vec!["use std::fmt;".into()]),
                remove: None,
                dry_run: Some(true),
            },
        )
        .unwrap();
        assert!(!r.written);
        assert_eq!(r.added, 1);
        assert!(!std::fs::read_to_string(dir.path().join("a.rs")).unwrap().contains("std::fmt"));
    }
}
