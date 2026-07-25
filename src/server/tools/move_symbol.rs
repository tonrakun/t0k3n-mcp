//! move_symbol — move a symbol from one file to another (Phase 15, opt-in write).
//!
//! Composes the extract side of delete_symbol with an end-of-file insert into
//! the destination (created if missing). Import fixups are best-effort: this
//! does not rewrite `use`/`import` lines, but it reports the files that
//! reference the symbol (via read_symbol_usages) so the caller knows what to
//! check. Opt-in write tool.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

use super::code::{ReadSymbolUsagesParams, read_symbol_usages};
use super::patch::parse_id;
use super::writes::unified_diff;
use crate::security::{rel_display, safe_path};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MoveSymbolParams {
    #[schemars(description = "Root-relative path of the source file")]
    pub src_path: String,
    #[schemars(description = "Skeleton ID of the symbol to move (from read_code on src_path)")]
    pub id: String,
    #[schemars(
        description = "Root-relative path of the destination file (created if it does not exist)"
    )]
    pub dest_path: String,
    #[schemars(
        description = "Symbol name: guards against stale line numbers and enables a reference-impact warning. Recommended."
    )]
    pub symbol_name: Option<String>,
    #[schemars(description = "true = return the would-be diffs without writing (default false)")]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct MoveSymbolResult {
    pub moved_lines: usize,
    pub dest_created: bool,
    pub src_diff: String,
    pub dest_diff: String,
    pub warnings: Vec<String>,
    pub written: bool,
}

pub fn move_symbol(root: &Path, params: MoveSymbolParams) -> anyhow::Result<MoveSymbolResult> {
    let src = safe_path(root, &params.src_path)?;
    let dest = safe_path(root, &params.dest_path)?;
    if src == dest {
        anyhow::bail!("src_path and dest_path are the same file");
    }

    let src_content = std::fs::read_to_string(&src)?;
    let (_, start, end) = parse_id(&params.id)
        .ok_or_else(|| anyhow::anyhow!("invalid id '{}' — expected 'kind:start-end'", params.id))?;

    let src_crlf = src_content.contains("\r\n");
    let src_trailing = src_content.ends_with('\n');
    let lines: Vec<&str> = src_content.lines().collect();
    if end > lines.len() {
        anyhow::bail!(
            "id range {}-{} exceeds file length {} — skeleton is stale, re-run read_code_skeleton",
            start,
            end,
            lines.len()
        );
    }

    let symbol_lines = &lines[start - 1..end];
    if let Some(name) = &params.symbol_name
        && !symbol_lines.iter().any(|l| l.contains(name.as_str()))
    {
        anyhow::bail!(
            "symbol_name '{}' not found in lines {}-{} — skeleton is stale, re-run read_code_skeleton",
            name,
            start,
            end
        );
    }
    let symbol_text = symbol_lines.join("\n");
    let moved_lines = symbol_lines.len();

    // ── Remove from source (with one trailing blank line) ──
    let mut cut_end = end;
    if cut_end < lines.len() && lines[cut_end].trim().is_empty() {
        cut_end += 1;
    }
    let mut new_src_lines: Vec<&str> = Vec::with_capacity(lines.len());
    new_src_lines.extend_from_slice(&lines[..start - 1]);
    new_src_lines.extend_from_slice(&lines[cut_end..]);
    let src_eol = if src_crlf { "\r\n" } else { "\n" };
    let mut new_src = new_src_lines.join(src_eol);
    if src_trailing && !new_src.is_empty() {
        new_src.push_str(src_eol);
    }
    let src_diff = unified_diff(&src_content, &new_src);

    // ── Append to destination (end of file), creating it if needed ──
    let dest_existed = dest.exists();
    let dest_content = if dest_existed {
        std::fs::read_to_string(&dest)?
    } else {
        String::new()
    };
    let dest_crlf = dest_content.contains("\r\n");
    let dest_eol = if dest_crlf || (!dest_existed && src_crlf) {
        "\r\n"
    } else {
        "\n"
    };
    let symbol_block = symbol_text.replace("\r\n", "\n").replace('\n', dest_eol);
    let new_dest = if dest_content.trim().is_empty() {
        format!("{symbol_block}{dest_eol}")
    } else {
        let base = dest_content.trim_end_matches(['\r', '\n']);
        format!("{base}{dest_eol}{dest_eol}{symbol_block}{dest_eol}")
    };
    let dest_diff = unified_diff(&dest_content, &new_dest);

    // ── Best-effort reference-impact warning ──
    let mut warnings = Vec::new();
    if let Some(name) = &params.symbol_name {
        if let Ok(usages) = read_symbol_usages(
            root,
            ReadSymbolUsagesParams {
                symbol: name.clone(),
                path: None,
            },
        ) {
            let src_rel = rel_display(root, &src);
            let dest_rel = rel_display(root, &dest);
            let mut refs: Vec<String> = usages
                .usages
                .into_iter()
                .map(|u| u.path)
                .filter(|p| p != &src_rel && p != &dest_rel)
                .collect();
            refs.sort();
            refs.dedup();
            if !refs.is_empty() {
                warnings.push(format!(
                    "imports/references not auto-updated — these files reference '{}': {}",
                    name,
                    refs.join(", ")
                ));
            }
        }
    } else {
        warnings.push(
            "pass symbol_name to get a reference-impact warning and stale-line guard".to_string(),
        );
    }

    let dry_run = params.dry_run.unwrap_or(false);
    if !dry_run {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, &new_dest)?;
        std::fs::write(&src, &new_src)?;
    }

    Ok(MoveSymbolResult {
        moved_lines,
        dest_created: !dest_existed,
        src_diff,
        dest_diff,
        warnings,
        written: !dry_run,
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
    fn moves_symbol_to_existing_dest() {
        let dir = setup(&[
            ("a.rs", "fn keep() {}\n\nfn moveme() {\n    work();\n}\n"),
            ("b.rs", "fn existing() {}\n"),
        ]);
        let r = move_symbol(
            dir.path(),
            MoveSymbolParams {
                src_path: "a.rs".into(),
                id: "function:3-5".into(),
                dest_path: "b.rs".into(),
                symbol_name: Some("moveme".into()),
                dry_run: None,
            },
        )
        .unwrap();
        assert!(r.written);
        assert!(!r.dest_created);
        let a = std::fs::read_to_string(dir.path().join("a.rs")).unwrap();
        let b = std::fs::read_to_string(dir.path().join("b.rs")).unwrap();
        assert!(!a.contains("moveme"));
        assert!(a.contains("fn keep()"));
        assert!(b.contains("fn existing()"));
        assert!(b.contains("fn moveme()"));
    }

    #[test]
    fn creates_dest_when_missing() {
        let dir = setup(&[("a.rs", "fn moveme() {\n    x();\n}\n")]);
        let r = move_symbol(
            dir.path(),
            MoveSymbolParams {
                src_path: "a.rs".into(),
                id: "function:1-3".into(),
                dest_path: "sub/new.rs".into(),
                symbol_name: Some("moveme".into()),
                dry_run: None,
            },
        )
        .unwrap();
        assert!(r.dest_created);
        let b = std::fs::read_to_string(dir.path().join("sub/new.rs")).unwrap();
        assert!(b.contains("fn moveme()"));
    }

    #[test]
    fn warns_about_referencing_files() {
        let dir = setup(&[
            ("a.rs", "fn moveme() {}\n"),
            ("c.rs", "fn caller() { moveme(); }\n"),
        ]);
        let r = move_symbol(
            dir.path(),
            MoveSymbolParams {
                src_path: "a.rs".into(),
                id: "function:1-1".into(),
                dest_path: "b.rs".into(),
                symbol_name: Some("moveme".into()),
                dry_run: Some(true),
            },
        )
        .unwrap();
        assert!(!r.written);
        assert!(r.warnings.iter().any(|w| w.contains("c.rs")));
    }

    #[test]
    fn stale_name_rejected() {
        let dir = setup(&[("a.rs", "fn a() {}\n"), ("b.rs", "\n")]);
        let r = move_symbol(
            dir.path(),
            MoveSymbolParams {
                src_path: "a.rs".into(),
                id: "function:1-1".into(),
                dest_path: "b.rs".into(),
                symbol_name: Some("nonexistent".into()),
                dry_run: None,
            },
        );
        assert!(r.is_err());
    }

    #[test]
    fn same_file_rejected() {
        let dir = setup(&[("a.rs", "fn a() {}\n")]);
        let r = move_symbol(
            dir.path(),
            MoveSymbolParams {
                src_path: "a.rs".into(),
                id: "function:1-1".into(),
                dest_path: "a.rs".into(),
                symbol_name: None,
                dry_run: None,
            },
        );
        assert!(r.is_err());
    }
}
