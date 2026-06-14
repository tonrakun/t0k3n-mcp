//! MCP Resources — exposes a workspace's key files (manifests, READMEs, and
//! conventional entry points) as `t0k3n://<path>` resources so resource-aware
//! clients can list and read them via the MCP `resources/*` methods. Kept
//! decoupled from rmcp types: this module returns plain data, and the
//! ServerHandler builds the protocol structs.

use ignore::WalkBuilder;
use std::path::Path;

use crate::security::{rel_display, safe_path};

pub const URI_PREFIX: &str = "t0k3n://";

/// Hard cap so a large repo never floods the resource list.
const MAX_RESOURCES: usize = 30;
const MAX_WALK_DEPTH: usize = 3;

/// Conventional manifests / docs surfaced verbatim when present at the root.
const ROOT_FILES: &[&str] = &[
    "README.md",
    "README.ja.md",
    "Cargo.toml",
    "package.json",
    "pyproject.toml",
    "go.mod",
    "requirements.txt",
    "tsconfig.json",
    "CLAUDE.md",
];

/// Source-file stems that conventionally mark an entry point.
const ENTRY_STEMS: &[&str] = &[
    "main", "lib", "index", "app", "server", "mod", "__init__", "cli",
];

pub struct ResourceEntry {
    pub uri: String,
    pub name: String,
    pub rel: String,
    pub mime: String,
    pub size: u32,
}

fn mime_for(rel: &str) -> &'static str {
    let ext = rel.rsplit('.').next().unwrap_or("");
    match ext {
        "md" => "text/markdown",
        "json" => "application/json",
        "toml" => "application/toml",
        "rs" => "text/x-rust",
        "ts" | "tsx" => "application/typescript",
        "js" | "jsx" => "application/javascript",
        "py" => "text/x-python",
        "go" => "text/x-go",
        _ => "text/plain",
    }
}

fn entry(root: &Path, path: &Path) -> Option<ResourceEntry> {
    let rel = rel_display(root, path).replace('\\', "/");
    let size = std::fs::metadata(path).ok().map(|m| m.len()).unwrap_or(0) as u32;
    Some(ResourceEntry {
        uri: format!("{URI_PREFIX}{rel}"),
        name: rel.clone(),
        mime: mime_for(&rel).to_string(),
        rel,
        size,
    })
}

/// List the workspace's key files as resources (manifests/docs first, then
/// conventional entry points), de-duplicated and capped.
pub fn list_workspace_resources(root: &Path) -> Vec<ResourceEntry> {
    let mut out: Vec<ResourceEntry> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Root manifests / docs.
    for name in ROOT_FILES {
        let path = root.join(name);
        if path.is_file()
            && let Some(e) = entry(root, &path)
            && seen.insert(e.rel.clone())
        {
            out.push(e);
        }
    }

    // Conventional entry-point source files (shallow walk, gitignore-aware).
    for dirent in WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .max_depth(Some(MAX_WALK_DEPTH))
        .build()
        .flatten()
    {
        if out.len() >= MAX_RESOURCES {
            break;
        }
        let path = dirent.path();
        if !path.is_file() {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if !ENTRY_STEMS.contains(&stem) {
            continue;
        }
        if let Some(e) = entry(root, path)
            && seen.insert(e.rel.clone())
        {
            out.push(e);
        }
    }

    out.truncate(MAX_RESOURCES);
    out
}

/// Resolve a `t0k3n://` URI to a safe absolute path under `root`. Returns None
/// for a foreign scheme or a path-traversal attempt.
pub fn resolve_uri(root: &Path, uri: &str) -> Option<std::path::PathBuf> {
    let rel = uri.strip_prefix(URI_PREFIX)?;
    safe_path(root, rel).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn lists_manifests_and_entry_points() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "Cargo.toml", "[package]\n");
        write(dir.path(), "README.md", "# hi\n");
        write(dir.path(), "src/main.rs", "fn main() {}\n");
        write(dir.path(), "src/helper.rs", "fn h() {}\n"); // not an entry stem

        let res = list_workspace_resources(dir.path());
        let names: Vec<&str> = res.iter().map(|r| r.rel.as_str()).collect();
        assert!(names.contains(&"Cargo.toml"));
        assert!(names.contains(&"README.md"));
        assert!(names.iter().any(|n| n.ends_with("main.rs")));
        assert!(!names.iter().any(|n| n.ends_with("helper.rs")));
        // URIs use the t0k3n scheme.
        assert!(res.iter().all(|r| r.uri.starts_with(URI_PREFIX)));
    }

    #[test]
    fn resolve_uri_rejects_foreign_scheme_and_traversal() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.rs", "x");
        assert!(resolve_uri(dir.path(), "t0k3n://a.rs").is_some());
        assert!(resolve_uri(dir.path(), "file:///a.rs").is_none());
        assert!(resolve_uri(dir.path(), "t0k3n://../../etc/passwd").is_none());
    }

    #[test]
    fn mime_detection() {
        assert_eq!(mime_for("README.md"), "text/markdown");
        assert_eq!(mime_for("Cargo.toml"), "application/toml");
        assert_eq!(mime_for("src/main.rs"), "text/x-rust");
    }
}
