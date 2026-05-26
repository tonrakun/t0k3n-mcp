use std::path::{Path, PathBuf};

use ignore::Walk;
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::security::safe_path;
use super::fs::estimate_tokens;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadCodeDepsParams {
    #[schemars(description = "Root-relative path to the code file")]
    pub path: String,
    #[schemars(description = "Direction: \"imports\" | \"imported_by\" | \"both\" (default: \"both\")")]
    pub direction: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DepImport {
    pub raw: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved: Option<String>,
    pub symbols: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReadCodeDepsResult {
    pub path: String,
    pub language: String,
    pub imports: Vec<DepImport>,
    pub imported_by: Vec<String>,
    pub token_count: usize,
}

pub fn read_code_deps(root: &Path, params: ReadCodeDepsParams) -> anyhow::Result<ReadCodeDepsResult> {
    let abs_path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&abs_path)?;
    let ext = abs_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let direction = params.direction.as_deref().unwrap_or("both");

    let language = lang_from_ext(ext).to_string();

    let imports = if direction == "imports" || direction == "both" {
        extract_imports(&content, ext, &abs_path, root)
    } else {
        vec![]
    };

    let imported_by = if direction == "imported_by" || direction == "both" {
        scan_imported_by(root, &abs_path, ext)
    } else {
        vec![]
    };

    let json = serde_json::json!({ "imports": imports, "imported_by": imported_by });
    let token_count = estimate_tokens(&json.to_string());

    Ok(ReadCodeDepsResult { path: params.path, language, imports, imported_by, token_count })
}

fn lang_from_ext(ext: &str) -> &'static str {
    match ext {
        "rs" => "rust",
        "py" => "python",
        "js" | "jsx" => "javascript",
        "ts" | "tsx" => "typescript",
        "go" => "go",
        _ => "unknown",
    }
}

// ─── import extraction ────────────────────────────────────────────────────────

fn extract_imports(content: &str, ext: &str, file_path: &Path, root: &Path) -> Vec<DepImport> {
    match ext {
        "rs" => extract_rust_imports(content),
        "py" => extract_python_imports(content),
        "js" | "jsx" | "ts" | "tsx" => extract_js_imports(content, file_path, root),
        "go" => extract_go_imports(content),
        _ => vec![],
    }
}

fn extract_rust_imports(content: &str) -> Vec<DepImport> {
    let re = Regex::new(r"^use\s+([^;]+);").unwrap();
    let mut imports = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(cap) = re.captures(trimmed) {
            let raw = cap[1].trim().to_string();
            let symbols = rust_symbols_from_path(&raw);
            imports.push(DepImport { raw, resolved: None, symbols });
        }
    }
    imports
}

fn rust_symbols_from_path(path: &str) -> Vec<String> {
    if let Some(pos) = path.rfind("::") {
        let tail = &path[pos + 2..];
        if tail.starts_with('{') {
            return tail
                .trim_matches(|c| c == '{' || c == '}')
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
        if tail != "*" {
            return vec![tail.to_string()];
        }
    }
    vec![path.to_string()]
}

fn extract_python_imports(content: &str) -> Vec<DepImport> {
    let import_re = Regex::new(r"^import\s+(\S+)").unwrap();
    let from_re = Regex::new(r"^from\s+(\S+)\s+import\s+(.+)").unwrap();
    let mut imports = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(cap) = from_re.captures(trimmed) {
            let module = cap[1].trim().to_string();
            let names_raw = cap[2].trim();
            let symbols: Vec<String> = names_raw
                .trim_matches(|c| c == '(' || c == ')')
                .split(',')
                .map(|s| s.trim().split_whitespace().next().unwrap_or("").to_string())
                .filter(|s| !s.is_empty())
                .collect();
            imports.push(DepImport {
                raw: format!("from {module} import {names_raw}"),
                resolved: None,
                symbols,
            });
        } else if let Some(cap) = import_re.captures(trimmed) {
            let module = cap[1].trim().to_string();
            imports.push(DepImport { raw: module.clone(), resolved: None, symbols: vec![module] });
        }
    }
    imports
}

fn extract_js_imports(content: &str, file_path: &Path, root: &Path) -> Vec<DepImport> {
    // ES import: import { A, B } from './foo'
    let import_re = Regex::new(
        r#"(?m)^import\s+(?:[*]\s+as\s+\w+|\{([^}]*)\}|(\w+)(?:\s*,\s*\{([^}]*)\})?)\s+from\s+['"]([^'"]+)['"]"#,
    )
    .unwrap();
    // CommonJS: const { A } = require('./foo')
    let require_re =
        Regex::new(r#"(?:const|let|var)\s+(?:\{([^}]*)\}|(\w+))\s*=\s*require\(['"]([^'"]+)['"]\)"#)
            .unwrap();

    let dir = file_path.parent().unwrap_or(root);
    let mut imports = Vec::new();

    for cap in import_re.captures_iter(content) {
        let src = cap[4].trim().to_string();
        let symbols = cap
            .get(1)
            .or_else(|| cap.get(3))
            .map(|m| {
                m.as_str()
                    .split(',')
                    .map(|s| s.trim().split_whitespace().next().unwrap_or("").to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_else(|| cap.get(2).map(|m| vec![m.as_str().to_string()]).unwrap_or_default());
        let resolved = if src.starts_with('.') {
            resolve_js_path(dir, &src, root)
        } else {
            None
        };
        imports.push(DepImport { raw: src, resolved, symbols });
    }

    for cap in require_re.captures_iter(content) {
        let src = cap[3].trim().to_string();
        let symbols = cap
            .get(1)
            .map(|m| {
                m.as_str()
                    .split(',')
                    .map(|s| s.trim().split_whitespace().next().unwrap_or("").to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_else(|| cap.get(2).map(|m| vec![m.as_str().to_string()]).unwrap_or_default());
        let resolved = if src.starts_with('.') {
            resolve_js_path(dir, &src, root)
        } else {
            None
        };
        imports.push(DepImport { raw: src, resolved, symbols });
    }

    imports
}

fn resolve_js_path(dir: &Path, src: &str, root: &Path) -> Option<String> {
    let base = dir.join(src);
    let candidates: &[&str] = &["", ".ts", ".tsx", ".js", ".jsx"];
    for ext in candidates {
        let candidate = if ext.is_empty() {
            base.clone()
        } else {
            PathBuf::from(format!("{}{}", base.display(), ext))
        };
        if candidate.exists() {
            return candidate
                .strip_prefix(root)
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"));
        }
    }
    for idx in &["index.ts", "index.tsx", "index.js", "index.jsx"] {
        let candidate = base.join(idx);
        if candidate.exists() {
            return candidate
                .strip_prefix(root)
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"));
        }
    }
    None
}

fn extract_go_imports(content: &str) -> Vec<DepImport> {
    let block_path_re = Regex::new(r#""([^"]+)""#).unwrap();
    let single_re = Regex::new(r#"^import\s+"([^"]+)""#).unwrap();
    let mut imports = Vec::new();
    let mut in_block = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "import (" {
            in_block = true;
            continue;
        }
        if in_block {
            if trimmed == ")" {
                in_block = false;
                continue;
            }
            if let Some(cap) = block_path_re.captures(trimmed) {
                let path = cap[1].to_string();
                let name = path.split('/').last().unwrap_or(&path).to_string();
                imports.push(DepImport { raw: path, resolved: None, symbols: vec![name] });
            }
        } else if let Some(cap) = single_re.captures(trimmed) {
            let path = cap[1].to_string();
            let name = path.split('/').last().unwrap_or(&path).to_string();
            imports.push(DepImport { raw: path, resolved: None, symbols: vec![name] });
        }
    }
    imports
}

// ─── imported_by scan ─────────────────────────────────────────────────────────

fn scan_imported_by(root: &Path, target: &Path, target_ext: &str) -> Vec<String> {
    let rel_target = match target.strip_prefix(root) {
        Ok(p) => p,
        Err(_) => return vec![],
    };
    let stem = match target.file_stem().and_then(|s| s.to_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return vec![],
    };

    let search_exts: &[&str] = match target_ext {
        "rs" => &["rs"],
        "py" => &["py"],
        "js" | "jsx" => &["js", "jsx", "ts", "tsx"],
        "ts" | "tsx" => &["ts", "tsx", "js", "jsx"],
        "go" => &["go"],
        _ => return vec![],
    };

    let mut results = Vec::new();

    for entry in Walk::new(root).flatten() {
        let path = entry.path();
        if path == target {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !search_exts.contains(&ext) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else { continue };
        if content.contains(stem.as_str()) && references_target(&content, &stem, ext, rel_target) {
            if let Ok(rel) = path.strip_prefix(root) {
                results.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
        if results.len() >= 200 {
            break;
        }
    }

    results
}

fn references_target(content: &str, stem: &str, ext: &str, _rel_target: &Path) -> bool {
    match ext {
        "rs" => content.lines().any(|l| {
            let l = l.trim();
            (l.starts_with("use ") || l.starts_with("mod ")) && l.contains(stem)
        }),
        "py" => content.lines().any(|l| {
            let l = l.trim();
            (l.starts_with("import ") || l.starts_with("from ")) && l.contains(stem)
        }),
        "js" | "jsx" | "ts" | "tsx" => content.lines().any(|l| {
            (l.contains("import ") || l.contains("require(")) && l.contains(stem)
        }),
        "go" => content.lines().any(|l| l.contains('"') && l.contains(stem)),
        _ => false,
    }
}
