use std::collections::HashSet;
use std::path::Path;

use ignore::WalkBuilder;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::code::{
    ReadCallGraphParams, ReadSymbolUsagesParams, ReadSymbolUsagesResult,
    read_call_graph, read_symbol_usages,
};
use super::fs::estimate_tokens;
use crate::security::{rel_display, scoped_root};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadRefactorImpactParams {
    #[schemars(description = "Symbol name (function, class, type, etc.) to analyze")]
    pub symbol: String,
    #[schemars(description = "Root-relative path of the file that defines the symbol. Omit to search entire workspace.")]
    pub path: Option<String>,
    #[schemars(description = "Include test files that reference the symbol (default: true)")]
    pub include_tests: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct TestRefEntry {
    pub path: String,
    pub line: usize,
}

#[derive(Debug, Serialize)]
pub struct ReadRefactorImpactResult {
    pub symbol: String,
    pub definition_file: Option<String>,
    pub definition_line: Option<usize>,
    /// Direct callers from call graph (within definition file)
    pub direct_callers: Vec<String>,
    /// Direct callees from call graph
    pub direct_callees: Vec<String>,
    /// All files that reference the symbol
    pub referenced_in: Vec<String>,
    /// Total reference count across workspace
    pub total_references: usize,
    /// Test files that reference the symbol
    pub test_files: Vec<TestRefEntry>,
    /// Estimated number of files that need review on change
    pub blast_radius: usize,
    pub token_count: usize,
}

pub fn read_refactor_impact(
    root: &Path,
    params: ReadRefactorImpactParams,
) -> anyhow::Result<ReadRefactorImpactResult> {
    let include_tests = params.include_tests.unwrap_or(true);
    let symbol = params.symbol.clone();

    // 1. Find definition file and line
    let (def_file, def_line) = find_definition(root, &symbol, params.path.as_deref());

    // 2. Get all symbol usages across workspace
    let usages_params = ReadSymbolUsagesParams {
        symbol: symbol.clone(),
        path: None,
    };
    let usages = read_symbol_usages(root, usages_params)
        .unwrap_or_else(|_| ReadSymbolUsagesResult {
            symbol: symbol.clone(),
            usages: vec![],
            total: 0,
            truncated: false,
            token_count: 0,
        });

    let total_references = usages.total;

    // Unique files referencing the symbol
    let mut referenced_files: HashSet<String> = HashSet::new();
    let mut test_files: Vec<TestRefEntry> = Vec::new();

    for u in &usages.usages {
        referenced_files.insert(u.path.clone());
        if include_tests && is_test_file(&u.path) {
            test_files.push(TestRefEntry {
                path: u.path.clone(),
                line: u.line,
            });
        }
    }

    // Deduplicate test files by path
    test_files.dedup_by(|a, b| a.path == b.path);

    let mut referenced_in: Vec<String> = referenced_files.into_iter().collect();
    referenced_in.sort();

    // 3. Get call graph if we know the definition file
    let (mut direct_callers, mut direct_callees) = (vec![], vec![]);
    if let Some(ref df) = def_file {
        // Build a synthetic function_id — best effort
        let fn_id = if let Some(line) = def_line {
            format!("function:{}-{}", line, line + 10)
        } else {
            format!("function:0-0")
        };
        let cg_params = ReadCallGraphParams {
            path: df.clone(),
            function_id: fn_id,
            depth: Some(0),
        };
        if let Ok(cg) = read_call_graph(root, cg_params) {
            direct_callers = cg.called_by_in_file;
            direct_callees = cg.calls;
        }
    }

    let blast_radius = referenced_in.len();

    let result_json = serde_json::json!({
        "symbol": symbol,
        "definition_file": def_file,
        "referenced_in": referenced_in,
        "test_files": test_files,
    });
    let token_count = estimate_tokens(&result_json.to_string());

    Ok(ReadRefactorImpactResult {
        symbol,
        definition_file: def_file,
        definition_line: def_line,
        direct_callers,
        direct_callees,
        referenced_in,
        total_references,
        test_files,
        blast_radius,
        token_count,
    })
}

fn find_definition(
    root: &Path,
    symbol: &str,
    hint_path: Option<&str>,
) -> (Option<String>, Option<usize>) {
    let search_root = scoped_root(root, hint_path).unwrap_or_else(|_| root.to_path_buf());

    // Definition patterns: `fn symbol`, `class symbol`, `struct symbol`, `def symbol`, etc.
    let patterns: Vec<String> = vec![
        format!("fn {}(", symbol),
        format!("fn {} (", symbol),
        format!("fn {}<", symbol),
        format!("class {} ", symbol),
        format!("class {}(", symbol),
        format!("class {}:", symbol),
        format!("struct {} ", symbol),
        format!("struct {}{{", symbol),
        format!("def {}(", symbol),
        format!("def {} (", symbol),
        format!("function {}(", symbol),
        format!("function {} (", symbol),
        format!("func {}(", symbol),
        format!("func {} (", symbol),
        format!("type {} =", symbol),
        format!("interface {} ", symbol),
        format!("trait {} ", symbol),
        format!("enum {} ", symbol),
    ];

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
        if !["rs", "py", "js", "jsx", "ts", "tsx", "go", "java", "rb", "cs", "php", "kt", "swift"].contains(&ext) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let rel = rel_display(root, path);

        for (line_idx, line) in content.lines().enumerate() {
            for pat in &patterns {
                if line.contains(pat.as_str()) {
                    return (Some(rel), Some(line_idx + 1));
                }
            }
        }
    }
    (None, None)
}

fn is_test_file(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.contains("/test")
        || lower.contains("/tests/")
        || lower.contains("/spec/")
        || lower.contains("_test.")
        || lower.contains(".test.")
        || lower.contains(".spec.")
        || lower.ends_with("_test.rs")
        || lower.ends_with("_test.go")
        || lower.ends_with("_spec.rb")
}
