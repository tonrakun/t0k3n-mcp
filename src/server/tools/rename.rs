use ignore::WalkBuilder;
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

use super::code::CODE_EXTENSIONS;
use super::fs::estimate_tokens;
use crate::security::{rel_display, scoped_root};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RenameSymbolParams {
    #[schemars(
        description = "Current symbol name to rename. Whole-identifier match (same detection basis as read_symbol_usages) — substrings are never touched."
    )]
    pub symbol: String,
    #[schemars(description = "New symbol name. Must be a valid identifier.")]
    pub new_name: String,
    #[schemars(
        description = "Restrict the rename to this file or directory (root-relative). Omit to rename across the whole workspace."
    )]
    pub path: Option<String>,
    #[schemars(
        description = "true = report affected files and edits without writing (default: false). Run once with dry_run to preview scope."
    )]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct RenameEdit {
    pub line: usize,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Serialize)]
pub struct RenameFileChange {
    pub path: String,
    pub edits: Vec<RenameEdit>,
}

#[derive(Debug, Serialize)]
pub struct RenameSymbolResult {
    pub applied: bool,
    pub files_changed: usize,
    pub occurrences: usize,
    pub changes: Vec<RenameFileChange>,
    pub token_count: usize,
}

/// A valid identifier starts with a letter or underscore and contains only
/// alphanumerics / underscores. Keeps the rename from producing broken source.
fn is_valid_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

pub fn rename_symbol(
    root: &Path,
    params: RenameSymbolParams,
) -> anyhow::Result<RenameSymbolResult> {
    if params.symbol.is_empty() {
        anyhow::bail!("symbol は空にできません");
    }
    if params.new_name.is_empty() {
        anyhow::bail!("new_name は空にできません");
    }
    if !is_valid_identifier(&params.new_name) {
        anyhow::bail!(
            "new_name '{}' は有効な識別子ではありません",
            params.new_name
        );
    }
    if params.symbol == params.new_name {
        anyhow::bail!("symbol と new_name が同一です");
    }

    let start = scoped_root(root, params.path.as_deref())?;
    let dry_run = params.dry_run.unwrap_or(false);

    let pattern = format!(r"\b{}\b", regex::escape(&params.symbol));
    let re = Regex::new(&pattern)
        .map_err(|e| anyhow::anyhow!("Invalid symbol '{}': {}", params.symbol, e))?;

    let mut changes: Vec<RenameFileChange> = Vec::new();
    let mut occurrences = 0usize;

    for entry in WalkBuilder::new(&start)
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .build()
        .flatten()
    {
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !CODE_EXTENSIONS.contains(&ext) {
            continue;
        }

        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        if !re.is_match(&content) {
            continue;
        }

        let uses_crlf = content.contains("\r\n");
        let had_trailing_newline = content.ends_with('\n');
        let lines: Vec<&str> = content.lines().collect();

        let mut edits: Vec<RenameEdit> = Vec::new();
        let mut new_lines: Vec<String> = Vec::with_capacity(lines.len());
        for (i, line) in lines.iter().enumerate() {
            let count = re.find_iter(line).count();
            if count > 0 {
                let replaced = re.replace_all(line, params.new_name.as_str()).into_owned();
                occurrences += count;
                edits.push(RenameEdit {
                    line: i + 1,
                    before: (*line).to_string(),
                    after: replaced.clone(),
                });
                new_lines.push(replaced);
            } else {
                new_lines.push((*line).to_string());
            }
        }

        if edits.is_empty() {
            continue;
        }

        let rel = rel_display(root, path);
        if !dry_run {
            let eol = if uses_crlf { "\r\n" } else { "\n" };
            let mut new_content = new_lines.join(eol);
            if had_trailing_newline {
                new_content.push_str(eol);
            }
            std::fs::write(path, &new_content)?;
        }
        changes.push(RenameFileChange { path: rel, edits });
    }

    let files_changed = changes.len();
    let json = serde_json::to_string(&changes).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(RenameSymbolResult {
        applied: !dry_run && files_changed > 0,
        files_changed,
        occurrences,
        changes,
        token_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, content) in files {
            let path = dir.path().join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(path, content).unwrap();
        }
        dir
    }

    #[test]
    fn renames_across_files() {
        let dir = setup(&[
            ("a.rs", "fn old_name() {}\nlet x = old_name();\n"),
            ("b.rs", "// uses old_name\nold_name();\n"),
        ]);
        let result = rename_symbol(
            dir.path(),
            RenameSymbolParams {
                symbol: "old_name".into(),
                new_name: "new_name".into(),
                path: None,
                dry_run: Some(false),
            },
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.files_changed, 2);
        let a = std::fs::read_to_string(dir.path().join("a.rs")).unwrap();
        assert!(a.contains("fn new_name()"));
        assert!(!a.contains("old_name"));
    }

    #[test]
    fn does_not_touch_substrings() {
        let dir = setup(&[("a.rs", "let old_name_extended = old_name;\n")]);
        rename_symbol(
            dir.path(),
            RenameSymbolParams {
                symbol: "old_name".into(),
                new_name: "renamed".into(),
                path: None,
                dry_run: None,
            },
        )
        .unwrap();
        let a = std::fs::read_to_string(dir.path().join("a.rs")).unwrap();
        assert!(a.contains("old_name_extended"));
        assert!(a.contains("= renamed;"));
    }

    #[test]
    fn dry_run_does_not_write() {
        let dir = setup(&[("a.rs", "fn old_name() {}\n")]);
        let result = rename_symbol(
            dir.path(),
            RenameSymbolParams {
                symbol: "old_name".into(),
                new_name: "new_name".into(),
                path: None,
                dry_run: Some(true),
            },
        )
        .unwrap();
        assert!(!result.applied);
        assert_eq!(result.files_changed, 1);
        let a = std::fs::read_to_string(dir.path().join("a.rs")).unwrap();
        assert!(a.contains("old_name"));
    }

    #[test]
    fn invalid_new_name_rejected() {
        let dir = setup(&[("a.rs", "fn old_name() {}\n")]);
        let result = rename_symbol(
            dir.path(),
            RenameSymbolParams {
                symbol: "old_name".into(),
                new_name: "123bad".into(),
                path: None,
                dry_run: None,
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn preserves_crlf() {
        let dir = setup(&[("a.rs", "fn old_name() {}\r\nlet x = 1;\r\n")]);
        rename_symbol(
            dir.path(),
            RenameSymbolParams {
                symbol: "old_name".into(),
                new_name: "new_name".into(),
                path: None,
                dry_run: None,
            },
        )
        .unwrap();
        let a = std::fs::read_to_string(dir.path().join("a.rs")).unwrap();
        assert!(a.contains("\r\n"));
        assert!(a.contains("fn new_name()"));
    }
}
