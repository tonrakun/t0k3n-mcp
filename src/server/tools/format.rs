//! format_code — run the language's formatter on a file (Phase 15, opt-in write).
//!
//! Drives rustfmt / prettier / black / gofmt by file extension. Like
//! read_type_diagnostics, a missing formatter returns a non-error note with an
//! install hint rather than failing. dry_run formats a temp copy so it can show
//! the diff without touching the real file.
//!
//! Unlike the shell-string runner in diagnostics.rs, formatters here are spawned
//! with the path as a separate argument (via `cmd /C <prog> … <path>` on Windows
//! to resolve `.cmd` shims, or directly elsewhere) so paths with spaces — and
//! Rust's arg quoting — are handled correctly.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use super::diagnostics::looks_unavailable;
use super::fs::estimate_tokens;
use super::writes::unified_diff;
use crate::security::safe_path;

const TIMEOUT: Duration = Duration::from_secs(45);

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FormatCodeParams {
    #[schemars(description = "Root-relative path to the file to format")]
    pub path: String,
    #[schemars(
        description = "true = format a copy and return the diff without writing (default false)"
    )]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct FormatCodeResult {
    pub formatter: String,
    pub formatter_available: bool,
    pub changed: bool,
    pub diff: String,
    pub written: bool,
    pub note: Option<String>,
    pub token_count: usize,
}

/// (formatter name, program, in-place args) for a file extension.
fn formatter_for(ext: &str) -> Option<(&'static str, &'static str, &'static [&'static str])> {
    match ext {
        "rs" => Some(("rustfmt", "rustfmt", &[])),
        "ts" | "tsx" | "js" | "jsx" | "json" | "css" | "scss" | "md" | "html" | "yaml" | "yml" => {
            Some(("prettier", "prettier", &["--write"]))
        }
        "py" => Some(("black", "black", &["-q"])),
        "go" => Some(("gofmt", "gofmt", &["-w"])),
        _ => None,
    }
}

fn install_hint(formatter: &str) -> String {
    let how = match formatter {
        "rustfmt" => "rustup component add rustfmt",
        "prettier" => "npm i -g prettier (or add it as a devDependency)",
        "black" => "pip install black",
        "gofmt" => "install the Go toolchain (go.dev/dl)",
        _ => "install the formatter",
    };
    format!("{formatter} not found (skipped). {how}.")
}

/// Spawn the formatter with `target` as a discrete argument. Returns stderr;
/// on Windows we go through `cmd /C` so `.cmd`/`.bat` shims (e.g. prettier)
/// resolve, but each argument stays separate so quoting is correct.
fn run_formatter(program: &str, extra: &[&str], target: &str, cwd: &Path) -> String {
    let mut cmd = {
        #[cfg(windows)]
        {
            let mut c = Command::new("cmd");
            c.arg("/C").arg(program).args(extra).arg(target);
            c
        }
        #[cfg(not(windows))]
        {
            let mut c = Command::new(program);
            c.args(extra).arg(target);
            c
        }
    };
    cmd.current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return format!("spawn failed: {e}"),
    };
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(TIMEOUT) {
        Ok(Ok(out)) => String::from_utf8_lossy(&out.stderr).into_owned(),
        Ok(Err(e)) => format!("wait failed: {e}"),
        Err(_) => "(timed out)".to_string(),
    }
}

pub fn format_code(root: &Path, params: FormatCodeParams) -> anyhow::Result<FormatCodeResult> {
    let path = safe_path(root, &params.path)?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let (formatter, program, extra) = formatter_for(ext)
        .ok_or_else(|| anyhow::anyhow!("no formatter configured for .{ext} files"))?;

    let original = std::fs::read_to_string(&path)?;
    let dry_run = params.dry_run.unwrap_or(false);

    // Format the real file in place, or a temp copy when previewing.
    let target: PathBuf = if dry_run {
        let tmp_dir = root.join(".t0k3n").join("fmt-tmp");
        std::fs::create_dir_all(&tmp_dir)?;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        let tmp = tmp_dir.join(format!("{nanos}-{base}"));
        std::fs::write(&tmp, &original)?;
        tmp
    } else {
        path.clone()
    };

    // safe_path canonicalizes to a `\\?\` verbatim path on Windows, which many
    // formatters cannot open — strip the prefix before passing it along.
    let target_str = target.to_string_lossy();
    let target_arg = target_str.strip_prefix(r"\\?\").unwrap_or(&target_str);

    let stderr = run_formatter(program, extra, target_arg, root);

    if looks_unavailable(&stderr) {
        if dry_run {
            let _ = std::fs::remove_file(&target);
        }
        return Ok(FormatCodeResult {
            formatter: formatter.to_string(),
            formatter_available: false,
            changed: false,
            diff: String::new(),
            written: false,
            note: Some(install_hint(formatter)),
            token_count: 0,
        });
    }

    let formatted = std::fs::read_to_string(&target).unwrap_or_else(|_| original.clone());
    if dry_run {
        let _ = std::fs::remove_file(&target);
    }

    let changed = formatted != original;
    let diff = if changed {
        unified_diff(&original, &formatted)
    } else {
        String::new()
    };

    // The formatter wrote the real file in place when !dry_run.
    let written = !dry_run && changed;

    // Surface a formatter error (e.g. syntax error) that prevented changes.
    let note = if !changed && !stderr.trim().is_empty() {
        Some(stderr.trim().to_string())
    } else {
        None
    };

    let token_count = estimate_tokens(&diff);
    Ok(FormatCodeResult {
        formatter: formatter.to_string(),
        formatter_available: true,
        changed,
        diff,
        written,
        note,
        token_count,
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
    fn formatter_for_maps_known_extensions() {
        assert_eq!(formatter_for("rs").unwrap().0, "rustfmt");
        assert_eq!(formatter_for("ts").unwrap().0, "prettier");
        assert_eq!(formatter_for("py").unwrap().0, "black");
        assert_eq!(formatter_for("go").unwrap().0, "gofmt");
        assert!(formatter_for("txt").is_none());
    }

    #[test]
    fn unsupported_extension_errors() {
        let dir = setup("a.txt", "hello");
        let r = format_code(
            dir.path(),
            FormatCodeParams {
                path: "a.txt".into(),
                dry_run: None,
            },
        );
        assert!(r.is_err());
    }

    #[test]
    fn rustfmt_formats_or_skips_when_unavailable() {
        let dir = setup("a.rs", "fn  main( ){let x=1;}\n");
        let r = format_code(
            dir.path(),
            FormatCodeParams {
                path: "a.rs".into(),
                dry_run: None,
            },
        )
        .unwrap();
        if r.formatter_available {
            let out = std::fs::read_to_string(dir.path().join("a.rs")).unwrap();
            assert!(
                r.changed,
                "rustfmt should reformat messy code; note={:?} file={:?}",
                r.note, out
            );
            assert!(out.contains("fn main()"));
        } else {
            // No rustfmt in this environment — must be a clean non-error skip.
            assert!(!r.written);
            assert!(r.note.is_some());
        }
    }

    #[test]
    fn dry_run_does_not_write() {
        let dir = setup("a.rs", "fn  main( ){let x=1;}\n");
        let r = format_code(
            dir.path(),
            FormatCodeParams {
                path: "a.rs".into(),
                dry_run: Some(true),
            },
        )
        .unwrap();
        assert!(!r.written);
        // Original file is untouched regardless of formatter availability.
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.rs")).unwrap(),
            "fn  main( ){let x=1;}\n"
        );
    }
}
