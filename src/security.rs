use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    #[error("Path '{0}' is outside the workspace root")]
    OutsideRoot(String),
    #[error("Path traversal detected in '{0}'")]
    PathTraversal(String),
    #[error("Symlink '{0}' points outside the workspace root")]
    SymlinkEscape(String),
    #[error(
        "Absolute path '{0}' is neither inside the workspace root nor a t0k3n temporary file"
    )]
    AbsoluteNotAllowed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Resolve a root-relative (or absolute) path and verify it stays within `root`.
/// Blocks path traversal (`../..`) and symlinks escaping the root.
pub fn safe_path(root: &Path, user_path: &str) -> Result<PathBuf, SecurityError> {
    let joined = if Path::new(user_path).is_absolute() {
        Path::new(user_path).to_path_buf()
    } else {
        root.join(user_path)
    };

    // Resolve without following symlinks first (to detect traversal in the path itself).
    // `..` underflow must be an error: with a relative root (e.g. "."), the normalized
    // root is empty and the starts_with check below is vacuous.
    let normalized = normalize_path_checked(&joined)
        .ok_or_else(|| SecurityError::PathTraversal(user_path.to_string()))?;

    // Check that the normalized path is under root before touching the filesystem
    let root_norm = normalize_path(root);
    if !normalized.starts_with(&root_norm) {
        return Err(SecurityError::OutsideRoot(user_path.to_string()));
    }

    // Now try to canonicalize (follows symlinks) if the path exists
    if normalized.exists() {
        let canonical = normalized.canonicalize()?;
        let root_canonical = root_norm
            .canonicalize()
            .unwrap_or_else(|_| root_norm.clone());
        if !canonical.starts_with(&root_canonical) {
            return Err(SecurityError::SymlinkEscape(user_path.to_string()));
        }
        return Ok(canonical);
    }

    Ok(normalized)
}

/// Resolve the optional `path` param of a directory-scanning tool into a walk root.
///
/// Validates like `safe_path` but returns the workspace-form path (`root.join(path)`)
/// instead of the canonical one: `canonicalize()` yields `\\?\`-prefixed verbatim
/// paths on Windows, which every walked entry inherits, so `strip_prefix(root)`
/// against the original root can never match — root-relative paths degrade to
/// absolute, and tools that feed them back into other readers return zero results.
pub fn scoped_root(root: &Path, user_path: Option<&str>) -> Result<PathBuf, SecurityError> {
    match user_path {
        Some(p) => {
            safe_path(root, p)?;
            Ok(root.join(p))
        }
        None => Ok(root.to_path_buf()),
    }
}

/// Workspace-relative display path with forward slashes.
///
/// Tries the root as given, then the canonicalized root (for paths that came out
/// of `safe_path`), and falls back to the path itself.
pub fn rel_display(root: &Path, path: &Path) -> String {
    let stripped = path
        .strip_prefix(root)
        .ok()
        .or_else(|| {
            let canon = root.canonicalize().ok()?;
            path.strip_prefix(canon).ok()
        })
        .unwrap_or(path);
    stripped.to_string_lossy().replace('\\', "/")
}

/// Like `safe_path`, plus one narrow exception: the `t0k3n-*.md` scratch files that
/// `convert_document` writes into the system temp directory. Everything else absolute
/// must still resolve inside `root`.
///
/// The exception used to be "any absolute path", which quietly made these tools the
/// one hole in the workspace sandbox.
pub fn safe_path_or_absolute(root: &Path, user_path: &str) -> Result<PathBuf, SecurityError> {
    let p = Path::new(user_path);
    if p.is_absolute() && !p.starts_with(root) && is_t0k3n_temp_file(p) {
        return Ok(p.to_path_buf());
    }
    safe_path(root, user_path).map_err(|e| match e {
        // Report the real reason for an absolute path that missed both allowances.
        SecurityError::OutsideRoot(_) if p.is_absolute() => {
            SecurityError::AbsoluteNotAllowed(user_path.to_string())
        }
        other => other,
    })
}

/// True for a `t0k3n-*` scratch file directly inside the system temp directory —
/// the only absolute location `safe_path_or_absolute` will step outside root for.
/// `..` is rejected first so `/tmp/../etc/passwd` cannot pose as a temp file.
fn is_t0k3n_temp_file(path: &Path) -> bool {
    let Some(normalized) = normalize_path_checked(path) else {
        return false;
    };
    if normalized != path {
        return false;
    }
    let temp = std::env::temp_dir();
    // Compare canonicalized temp roots where possible: on macOS /tmp and
    // /private/tmp name the same directory.
    let temp_canon = temp.canonicalize().unwrap_or_else(|_| temp.clone());
    let parent_canon = match normalized.parent() {
        Some(p) => p.canonicalize().unwrap_or_else(|_| p.to_path_buf()),
        None => return false,
    };
    if parent_canon != temp_canon {
        return false;
    }
    normalized
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with("t0k3n-"))
}

/// Normalize a path without hitting the filesystem (resolve `.` and `..`).
fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            c => out.push(c),
        }
    }
    out
}

/// Like `normalize_path`, but returns `None` when a `..` pops past the start
/// of the path (i.e. the path escapes its base).
fn normalize_path_checked(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    let mut depth: usize = 0;
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                if depth == 0 {
                    return None;
                }
                out.pop();
                depth -= 1;
            }
            std::path::Component::CurDir => {}
            std::path::Component::Normal(_) => {
                out.push(component);
                depth += 1;
            }
            c => out.push(c),
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests run with cwd = crate root, so "." and "src" exist on disk and
    // safe_path takes its canonicalize() branch (verbatim \\?\ paths on Windows).

    #[test]
    fn scoped_root_keeps_root_as_strippable_prefix() {
        let root = Path::new(".");
        let scoped = scoped_root(root, Some("src")).unwrap();
        assert_eq!(scoped, Path::new("./src"));
        // A path walked from the scoped root must strip back to root-relative form
        let walked = scoped.join("main.rs");
        assert_eq!(rel_display(root, &walked), "src/main.rs");
    }

    #[test]
    fn scoped_root_without_path_returns_root() {
        let root = Path::new(".");
        assert_eq!(scoped_root(root, None).unwrap(), root.to_path_buf());
    }

    #[test]
    fn scoped_root_rejects_traversal() {
        assert!(scoped_root(Path::new("."), Some("../outside")).is_err());
    }

    #[test]
    fn rel_display_handles_canonicalized_paths() {
        // safe_path returns canonicalized paths for existing files; rel_display
        // must still produce a root-relative path from them.
        let root = Path::new(".");
        let canonical = safe_path(root, "src/main.rs").unwrap();
        assert_eq!(rel_display(root, &canonical), "src/main.rs");
    }

    #[test]
    fn safe_path_or_absolute_allows_only_t0k3n_temp_files() {
        let root = tempfile::tempdir().unwrap();
        let temp = std::env::temp_dir();

        // The convert_document scratch file is allowed.
        let allowed = temp.join("t0k3n-abc123.md");
        assert!(
            safe_path_or_absolute(root.path(), allowed.to_str().unwrap()).is_ok(),
            "t0k3n-* temp files must stay readable"
        );

        // An unrelated file in the same temp dir is not.
        let other = temp.join("someone-elses-secret.md");
        assert!(safe_path_or_absolute(root.path(), other.to_str().unwrap()).is_err());

        // An arbitrary absolute path outside root is not.
        #[cfg(windows)]
        let outside = "C:\\Windows\\System32\\drivers\\etc\\hosts";
        #[cfg(not(windows))]
        let outside = "/etc/passwd";
        assert!(safe_path_or_absolute(root.path(), outside).is_err());

        // Traversal dressed up as a temp path is not.
        let traversal = temp.join("..").join("t0k3n-evil.md");
        assert!(safe_path_or_absolute(root.path(), traversal.to_str().unwrap()).is_err());
    }

    #[test]
    fn safe_path_or_absolute_still_accepts_paths_inside_root() {
        let root = tempfile::tempdir().unwrap();
        let inside = root.path().join("doc.md");
        assert!(safe_path_or_absolute(root.path(), inside.to_str().unwrap()).is_ok());
        assert!(safe_path_or_absolute(root.path(), "doc.md").is_ok());
    }

    #[test]
    fn rel_display_falls_back_to_path_itself() {
        let rel = rel_display(Path::new("src"), Path::new("other/file.rs"));
        assert_eq!(rel, "other/file.rs");
    }
}
