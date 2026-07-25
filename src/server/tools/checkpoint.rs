//! edit_checkpoint / rollback — a working-tree safety net for autonomous write
//! loops (Phase 15, opt-in write).
//!
//! In a git repo, edit_checkpoint snapshots tracked changes with
//! `git stash create` (which does NOT touch the working tree) and returns the
//! object SHA; rollback restores those paths with `git checkout <sha> -- .`.
//! Outside git, it falls back to copying the (gitignore-aware) workspace into
//! `.t0k3n/checkpoints/<id>/` and copying it back on rollback. The checkpoint_id
//! is self-contained, so no server state is needed between the two calls.
//!
//! Distinct from session_snapshot (which saves tool/work *state*); this snapshots
//! files on disk. Limitation: untracked files created after a checkpoint are not
//! removed by rollback.

use ignore::WalkBuilder;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

const MAX_COPY_FILES: usize = 2000;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EditCheckpointParams {
    #[schemars(description = "Optional human label for the checkpoint (informational)")]
    pub label: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EditCheckpointResult {
    pub checkpoint_id: String,
    pub strategy: String,
    pub files: usize,
    pub note: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RollbackParams {
    #[schemars(description = "checkpoint_id returned by edit_checkpoint")]
    pub checkpoint_id: String,
}

#[derive(Debug, Serialize)]
pub struct RollbackResult {
    pub strategy: String,
    pub restored: usize,
    pub note: String,
}

fn is_git(root: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(root)
        .output()
        .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "true")
        .unwrap_or(false)
}

fn run_git(args: &[&str], root: &Path) -> anyhow::Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| anyhow::anyhow!("git not available: {e}"))?;
    if !out.status.success() {
        anyhow::bail!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Files to snapshot in the copy fallback: gitignore-aware, excluding the t0k3n
/// state dir and any VCS dir.
fn snapshot_files(root: &Path) -> Vec<std::path::PathBuf> {
    WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            name != ".t0k3n" && name != ".git"
        })
        .build()
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| e.path().to_path_buf())
        .collect()
}

pub fn edit_checkpoint(
    root: &Path,
    params: EditCheckpointParams,
) -> anyhow::Result<EditCheckpointResult> {
    let label_suffix = params
        .label
        .as_deref()
        .filter(|l| !l.is_empty())
        .map(|l| format!(" [{l}]"))
        .unwrap_or_default();
    if is_git(root) {
        let created = run_git(&["stash", "create"], root)?;
        let sha = if created.is_empty() {
            // Clean tree: anchor to HEAD so rollback discards later edits.
            run_git(&["rev-parse", "HEAD"], root)?
        } else {
            created
        };
        if sha.is_empty() {
            anyhow::bail!("cannot checkpoint an empty git repository with no commits");
        }
        return Ok(EditCheckpointResult {
            checkpoint_id: format!("git:{sha}"),
            strategy: "git".to_string(),
            files: 0,
            note: format!(
                "tracked files only — untracked new files are not captured{label_suffix}"
            ),
        });
    }

    // Copy fallback.
    let files = snapshot_files(root);
    if files.len() > MAX_COPY_FILES {
        anyhow::bail!(
            "workspace too large for copy checkpoint ({} files > {}); use a git repo",
            files.len(),
            MAX_COPY_FILES
        );
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let id = format!("{nanos}");
    let cp_dir = root.join(".t0k3n").join("checkpoints").join(&id);
    let mut count = 0usize;
    for f in &files {
        let Ok(rel) = f.strip_prefix(root) else {
            continue;
        };
        let dest = cp_dir.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(f, &dest)?;
        count += 1;
    }
    Ok(EditCheckpointResult {
        checkpoint_id: format!("copy:{id}"),
        strategy: "copy".to_string(),
        files: count,
        note: format!(
            "copied gitignore-aware files; files created after this are not removed by rollback{label_suffix}"
        ),
    })
}

pub fn rollback(root: &Path, params: RollbackParams) -> anyhow::Result<RollbackResult> {
    if let Some(sha) = params.checkpoint_id.strip_prefix("git:") {
        run_git(&["checkout", sha, "--", "."], root)?;
        return Ok(RollbackResult {
            strategy: "git".to_string(),
            restored: 0,
            note: "tracked files restored to the checkpoint; untracked additions left in place"
                .to_string(),
        });
    }
    if let Some(id) = params.checkpoint_id.strip_prefix("copy:") {
        let cp_dir = root.join(".t0k3n").join("checkpoints").join(id);
        if !cp_dir.is_dir() {
            anyhow::bail!("checkpoint '{}' not found", params.checkpoint_id);
        }
        let mut restored = 0usize;
        for entry in WalkBuilder::new(&cp_dir).hidden(false).build().flatten() {
            let p = entry.path();
            if !p.is_file() {
                continue;
            }
            let Ok(rel) = p.strip_prefix(&cp_dir) else {
                continue;
            };
            let dest = root.join(rel);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(p, &dest)?;
            restored += 1;
        }
        return Ok(RollbackResult {
            strategy: "copy".to_string(),
            restored,
            note: "files restored from the copy checkpoint".to_string(),
        });
    }
    anyhow::bail!(
        "unknown checkpoint_id '{}' — expected a 'git:' or 'copy:' id from edit_checkpoint",
        params.checkpoint_id
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(args: &[&str], root: &Path) {
        let ok = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert!(ok, "git {args:?} failed");
    }

    #[test]
    fn copy_checkpoint_and_rollback_non_git() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "v1").unwrap();
        let cp = edit_checkpoint(dir.path(), EditCheckpointParams { label: None }).unwrap();
        assert_eq!(cp.strategy, "copy");
        assert!(cp.checkpoint_id.starts_with("copy:"));

        std::fs::write(dir.path().join("a.txt"), "v2-broken").unwrap();
        let rb = rollback(
            dir.path(),
            RollbackParams {
                checkpoint_id: cp.checkpoint_id,
            },
        )
        .unwrap();
        assert_eq!(rb.strategy, "copy");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "v1"
        );
    }

    #[test]
    fn git_checkpoint_and_rollback() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        if Command::new("git").arg("--version").output().is_err() {
            return; // no git in environment
        }
        git(&["init", "-q"], root);
        git(&["config", "user.email", "t@example.com"], root);
        git(&["config", "user.name", "t"], root);
        git(&["config", "core.autocrlf", "false"], root);
        std::fs::write(root.join("a.txt"), "committed\n").unwrap();
        git(&["add", "."], root);
        git(&["commit", "-q", "-m", "init"], root);

        // Make an uncommitted change, then checkpoint it.
        std::fs::write(root.join("a.txt"), "checkpoint-state\n").unwrap();
        let cp = edit_checkpoint(root, EditCheckpointParams { label: None }).unwrap();
        assert_eq!(cp.strategy, "git");

        // Break it further, then roll back to the checkpoint.
        std::fs::write(root.join("a.txt"), "broken\n").unwrap();
        rollback(
            root,
            RollbackParams {
                checkpoint_id: cp.checkpoint_id,
            },
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).unwrap(),
            "checkpoint-state\n"
        );
    }

    #[test]
    fn unknown_checkpoint_id_errors() {
        let dir = tempfile::tempdir().unwrap();
        let r = rollback(
            dir.path(),
            RollbackParams {
                checkpoint_id: "bogus".into(),
            },
        );
        assert!(r.is_err());
    }
}
