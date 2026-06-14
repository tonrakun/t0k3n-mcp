//! read_api_surface — extracts only the outward-facing symbols (Rust `pub`,
//! TS/JS `export`, Python `__all__` / non-underscore top-level, Go capitalized)
//! so the agent can understand a crate/package's external boundary without
//! reading bodies. Pairs with diff_schemas to flag breaking API changes.

use ignore::WalkBuilder;
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

use std::sync::LazyLock;

use super::fs::estimate_tokens;
use crate::security::{rel_display, scoped_root};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadApiSurfaceParams {
    #[schemars(description = "Restrict to this file or directory (root-relative). Omit for the whole workspace.")]
    pub path: Option<String>,
    #[schemars(description = "Include semi-public items too (Rust pub(crate)/pub(super)). Default false = only fully public API.")]
    pub include_crate_visible: Option<bool>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ApiItem {
    pub kind: String,
    pub name: String,
    pub signature: String,
    pub visibility: String,
}

#[derive(Debug, Serialize)]
pub struct ApiFile {
    pub path: String,
    pub language: String,
    pub items: Vec<ApiItem>,
}

#[derive(Debug, Serialize)]
pub struct ReadApiSurfaceResult {
    pub api: Vec<ApiFile>,
    pub token_count: usize,
}

const SUPPORTED: &[&str] = &["rs", "ts", "tsx", "js", "jsx", "py", "go"];

pub fn read_api_surface(
    root: &Path,
    params: ReadApiSurfaceParams,
) -> anyhow::Result<ReadApiSurfaceResult> {
    let start = scoped_root(root, params.path.as_deref())?;
    let include_crate = params.include_crate_visible.unwrap_or(false);

    let mut api: Vec<ApiFile> = Vec::new();
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
        if !SUPPORTED.contains(&ext) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };

        let (language, mut items) = extract(ext, &content);
        if !include_crate {
            items.retain(|i| i.visibility == "public");
        }
        if items.is_empty() {
            continue;
        }
        api.push(ApiFile {
            path: rel_display(root, path),
            language: language.to_string(),
            items,
        });
    }

    api.sort_by(|a, b| a.path.cmp(&b.path));
    let json = serde_json::to_string(&api).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadApiSurfaceResult { api, token_count })
}

fn extract(ext: &str, content: &str) -> (&'static str, Vec<ApiItem>) {
    match ext {
        "rs" => ("rust", extract_rust(content)),
        "ts" | "tsx" | "js" | "jsx" => ("typescript", extract_ts(content)),
        "py" => ("python", extract_python(content)),
        "go" => ("go", extract_go(content)),
        _ => ("unknown", Vec::new()),
    }
}

static RUST_PUB: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*pub(\([^)]*\))?\s+(?:async\s+|unsafe\s+|extern\s+(?:\x22[^\x22]*\x22\s+)?)*(fn|struct|enum|trait|type|const|static|mod)\s+(\w+)")
        .unwrap()
});

fn extract_rust(content: &str) -> Vec<ApiItem> {
    let mut items = Vec::new();
    for line in content.lines() {
        if let Some(cap) = RUST_PUB.captures(line) {
            let visibility = if cap.get(1).is_some() {
                "crate" // pub(crate) / pub(super) / pub(in ...)
            } else {
                "public"
            };
            let raw_kind = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            let kind = if raw_kind == "fn" { "function" } else { raw_kind };
            let name = cap.get(3).map(|m| m.as_str()).unwrap_or("").to_string();
            items.push(ApiItem {
                kind: kind.to_string(),
                name,
                signature: line.trim().trim_end_matches(['{', ' ']).trim().to_string(),
                visibility: visibility.to_string(),
            });
        }
    }
    items
}

static TS_EXPORT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*export\s+(?:default\s+)?(?:declare\s+)?(?:async\s+)?(?:abstract\s+)?(function\*?|class|interface|type|const|let|var|enum)\s+(\w+)")
        .unwrap()
});

fn extract_ts(content: &str) -> Vec<ApiItem> {
    let mut items = Vec::new();
    for line in content.lines() {
        if let Some(cap) = TS_EXPORT.captures(line) {
            let kind = cap
                .get(1)
                .map(|m| m.as_str())
                .unwrap_or("")
                .replace('*', "")
                .to_string();
            let name = cap.get(2).map(|m| m.as_str()).unwrap_or("").to_string();
            items.push(ApiItem {
                kind,
                name,
                signature: line.trim().trim_end_matches(['{', ' ']).trim().to_string(),
                visibility: "public".to_string(),
            });
        }
    }
    items
}

static PY_DEF: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(def|class|async def)\s+(\w+)").unwrap());
static PY_ALL_NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"['"]([A-Za-z_]\w*)['"]"#).unwrap());

fn extract_python(content: &str) -> Vec<ApiItem> {
    let mut items = Vec::new();

    // Top-level (column 0) defs/classes whose name does not start with '_'.
    for line in content.lines() {
        if let Some(cap) = PY_DEF.captures(line) {
            let kind = cap.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            let name = cap.get(2).map(|m| m.as_str()).unwrap_or("").to_string();
            if name.starts_with('_') {
                continue;
            }
            items.push(ApiItem {
                kind: kind.replace("async def", "function").replace("def", "function"),
                name,
                signature: line.trim().trim_end_matches(':').trim().to_string(),
                visibility: "public".to_string(),
            });
        }
    }

    // Names listed in __all__ that aren't already captured (e.g. re-exports).
    if let Some(all_block) = extract_all_block(content) {
        for cap in PY_ALL_NAME.captures_iter(&all_block) {
            let name = cap.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            if !name.is_empty() && !items.iter().any(|i| i.name == name) {
                items.push(ApiItem {
                    kind: "export".to_string(),
                    name: name.clone(),
                    signature: format!("__all__: {name}"),
                    visibility: "public".to_string(),
                });
            }
        }
    }

    items
}

/// Capture the text of an `__all__ = [ ... ]` assignment (possibly multi-line).
fn extract_all_block(content: &str) -> Option<String> {
    let start = content.find("__all__")?;
    let rest = &content[start..];
    let open = rest.find(['[', '('])?;
    let close_char = if rest.as_bytes()[open] == b'[' { ']' } else { ')' };
    let close = rest[open..].find(close_char)? + open;
    Some(rest[open..=close].to_string())
}

static GO_FUNC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^func\s+(\([^)]*\)\s+)?([A-Z]\w*)\s*[\(\[]").unwrap());
static GO_TYPE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^type\s+([A-Z]\w*)\s+(struct|interface|\w+)").unwrap());
static GO_VARCONST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(var|const)\s+([A-Z]\w*)").unwrap());

fn extract_go(content: &str) -> Vec<ApiItem> {
    let mut items = Vec::new();
    for line in content.lines() {
        if let Some(cap) = GO_FUNC.captures(line) {
            let kind = if cap.get(1).is_some() { "method" } else { "function" };
            let name = cap.get(2).map(|m| m.as_str()).unwrap_or("").to_string();
            items.push(ApiItem {
                kind: kind.to_string(),
                name,
                signature: line.trim().trim_end_matches(['{', ' ']).trim().to_string(),
                visibility: "public".to_string(),
            });
        } else if let Some(cap) = GO_TYPE.captures(line) {
            let name = cap.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            items.push(ApiItem {
                kind: "type".to_string(),
                name,
                signature: line.trim().trim_end_matches(['{', ' ']).trim().to_string(),
                visibility: "public".to_string(),
            });
        } else if let Some(cap) = GO_VARCONST.captures(line) {
            let kind = cap.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            let name = cap.get(2).map(|m| m.as_str()).unwrap_or("").to_string();
            items.push(ApiItem {
                kind,
                name,
                signature: line.trim().to_string(),
                visibility: "public".to_string(),
            });
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_pub_vs_crate() {
        let src = "pub fn open() {}\npub(crate) fn helper() {}\nfn private() {}\npub struct Cfg {\n}\n";
        let items = extract_rust(src);
        assert!(items.iter().any(|i| i.name == "open" && i.visibility == "public" && i.kind == "function"));
        assert!(items.iter().any(|i| i.name == "helper" && i.visibility == "crate"));
        assert!(items.iter().any(|i| i.name == "Cfg" && i.kind == "struct"));
        assert!(!items.iter().any(|i| i.name == "private"));
    }

    #[test]
    fn ts_exports() {
        let src = "export function foo() {}\nexport class Bar {}\nexport const X = 1;\nfunction hidden() {}\nexport interface I {}\n";
        let items = extract_ts(src);
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"foo"));
        assert!(names.contains(&"Bar"));
        assert!(names.contains(&"X"));
        assert!(names.contains(&"I"));
        assert!(!names.contains(&"hidden"));
    }

    #[test]
    fn python_toplevel_and_all() {
        let src = "def public_fn():\n    pass\ndef _private():\n    pass\nclass Public:\n    def method(self):\n        pass\n__all__ = ['public_fn', 'Reexported']\n";
        let items = extract_python(src);
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"public_fn"));
        assert!(names.contains(&"Public"));
        assert!(!names.contains(&"_private"));
        // method is indented (not top-level) → excluded
        assert!(!names.contains(&"method"));
        // __all__ re-export not defined locally → included
        assert!(names.contains(&"Reexported"));
    }

    #[test]
    fn go_exported_by_case() {
        let src = "func Exported() {}\nfunc unexported() {}\ntype Public struct {\n}\nfunc (r *R) Method() {}\nconst MaxN = 5\n";
        let items = extract_go(src);
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"Exported"));
        assert!(names.contains(&"Public"));
        assert!(names.contains(&"Method"));
        assert!(names.contains(&"MaxN"));
        assert!(!names.contains(&"unexported"));
    }

    #[test]
    fn end_to_end_filters_crate_visible() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("lib.rs"), "pub fn a() {}\npub(crate) fn b() {}\n").unwrap();

        let public_only = read_api_surface(dir.path(), ReadApiSurfaceParams { path: None, include_crate_visible: None }).unwrap();
        let names: Vec<&str> = public_only.api[0].items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["a"]);

        let with_crate = read_api_surface(dir.path(), ReadApiSurfaceParams { path: None, include_crate_visible: Some(true) }).unwrap();
        assert_eq!(with_crate.api[0].items.len(), 2);
    }
}
