use std::path::Path;

use ignore::WalkBuilder;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::code::{
    ReadCodeSkeletonParams, ReadSymbolUsagesParams, read_code_skeleton, read_symbol_usages,
};
use super::fs::estimate_tokens;
use crate::security::{rel_display, scoped_root};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadDeadCodeParams {
    #[schemars(
        description = "Root-relative file or directory to scan. Omit to scan entire workspace."
    )]
    pub path: Option<String>,
    #[schemars(
        description = "Also report public/exported symbols with no external callers (default: false)"
    )]
    pub include_exported: Option<bool>,
    #[schemars(description = "Maximum symbols to return (default: 50)")]
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct DeadCodeEntry {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub start_line: usize,
    pub end_line: usize,
    pub is_exported: bool,
    pub confidence: String, // "high" | "medium"
}

#[derive(Debug, Serialize)]
pub struct ReadDeadCodeResult {
    pub entries: Vec<DeadCodeEntry>,
    pub total_symbols_checked: usize,
    pub token_count: usize,
}

/// Symbols that are legitimately unreferenced by name.
const EXEMPT_NAMES: &[&str] = &[
    "main",
    "new",
    "default",
    "init",
    "setup",
    "teardown",
    "drop",
    "fmt",
    "clone",
    "debug",
    "display",
    "from",
    "into",
    "deref",
    "deref_mut",
    "index",
    "index_mut",
    "add",
    "sub",
    "mul",
    "div",
    "eq",
    "partial_eq",
    "hash",
    "serialize",
    "deserialize",
];

const CODE_EXTS: &[&str] = &[
    "rs", "py", "js", "jsx", "ts", "tsx", "go", "cpp", "cc", "cxx", "c", "java", "rb", "cs", "php",
    "kt", "swift",
];

pub fn read_dead_code(
    root: &Path,
    params: ReadDeadCodeParams,
) -> anyhow::Result<ReadDeadCodeResult> {
    let include_exported = params.include_exported.unwrap_or(false);
    let limit = params.limit.unwrap_or(50);

    let search_root =
        scoped_root(root, params.path.as_deref()).map_err(|e| anyhow::anyhow!("{e}"))?;

    // Collect all code files
    let mut code_files: Vec<String> = Vec::new();
    for entry in WalkBuilder::new(&search_root)
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
        if !CODE_EXTS.contains(&ext) {
            continue;
        }
        let rel = rel_display(root, path);
        code_files.push(rel);
    }

    let mut entries: Vec<DeadCodeEntry> = Vec::new();
    let mut total_symbols_checked: usize = 0;

    'files: for rel_path in &code_files {
        let skeleton_params = ReadCodeSkeletonParams {
            path: rel_path.clone(),
            include_blocks: Some(false),
        };
        let Ok(skeleton) = read_code_skeleton(root, skeleton_params) else {
            continue;
        };

        for item in &skeleton.skeleton {
            if !matches!(
                item.kind.as_str(),
                "function" | "method" | "class" | "struct" | "enum" | "trait"
            ) {
                continue;
            }
            total_symbols_checked += 1;

            // Skip exempt names
            if EXEMPT_NAMES.contains(&item.name.to_lowercase().as_str()) {
                continue;
            }
            // Skip test functions
            if item.name.starts_with("test_")
                || item.name.starts_with("Test")
                || item.name.ends_with("_test")
                || item.name.ends_with("Test")
            {
                continue;
            }
            // Skip very short names (likely operators/traits)
            if item.name.len() <= 2 {
                continue;
            }

            let is_exported = is_symbol_exported(&item.signature);
            if is_exported && !include_exported {
                continue;
            }

            // Count usages outside the definition file
            let usages_params = ReadSymbolUsagesParams {
                symbol: item.name.clone(),
                path: None,
            };
            let Ok(usages_result) = read_symbol_usages(root, usages_params) else {
                continue;
            };

            // Count references in OTHER files (not the definition file)
            let external_refs: usize = usages_result
                .usages
                .iter()
                .filter(|u| u.path != *rel_path)
                .count();

            // Count same-file refs outside the definition range
            let internal_refs: usize = usages_result
                .usages
                .iter()
                .filter(|u| {
                    u.path == *rel_path && (u.line < item.start_line || u.line > item.end_line)
                })
                .count();

            if external_refs > 0 || internal_refs > 0 {
                continue;
            }

            let confidence = if is_exported { "medium" } else { "high" };
            entries.push(DeadCodeEntry {
                path: rel_path.clone(),
                name: item.name.clone(),
                kind: item.kind.clone(),
                start_line: item.start_line,
                end_line: item.end_line,
                is_exported,
                confidence: confidence.to_string(),
            });

            if entries.len() >= limit {
                break 'files;
            }
        }
    }

    let json = serde_json::to_string(&entries).unwrap_or_default();
    let token_count = estimate_tokens(&json);

    Ok(ReadDeadCodeResult {
        entries,
        total_symbols_checked,
        token_count,
    })
}

fn is_symbol_exported(signature: &str) -> bool {
    signature.starts_with("pub ")
        || signature.starts_with("export ")
        || signature.starts_with("export default ")
        || signature.contains("public ")
        || signature.contains("Public ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: with a `path` param the walk root used to come back
    /// canonicalized (`\\?\` verbatim on Windows), so every collected rel
    /// path was unreadable and zero symbols were checked.
    #[test]
    fn scoped_scan_checks_symbols() {
        let params = ReadDeadCodeParams {
            path: Some("src".to_string()),
            include_exported: None,
            limit: None,
        };
        let result = read_dead_code(std::path::Path::new("."), params).unwrap();
        assert!(
            result.total_symbols_checked > 0,
            "scoped scan must check symbols"
        );
    }
}
