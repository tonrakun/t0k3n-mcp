use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    #[error("Path '{0}' is outside the workspace root")]
    OutsideRoot(String),
    #[allow(dead_code)]
    #[error("Path traversal detected in '{0}'")]
    PathTraversal(String),
    #[error("Symlink '{0}' points outside the workspace root")]
    SymlinkEscape(String),
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

    // Resolve without following symlinks first (to detect traversal in the path itself)
    let normalized = normalize_path(&joined);

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

/// Like `safe_path` but allows absolute paths anywhere (for tmp files from convert_document).
pub fn safe_path_or_absolute(root: &Path, user_path: &str) -> Result<PathBuf, SecurityError> {
    let p = Path::new(user_path);
    if p.is_absolute() {
        // Allow absolute paths (e.g., /tmp/t0k3n-*.md from convert_document)
        return Ok(p.to_path_buf());
    }
    safe_path(root, user_path)
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
