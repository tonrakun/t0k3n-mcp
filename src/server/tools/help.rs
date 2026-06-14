use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HelpParams {
    #[schemars(
        description = "Category filter: file/write/git/schema/web/notebook/test/log/text/memory/task/session/analysis/cmd/debug. Omit to list category names only; pass \"all\" for the full catalog."
    )]
    pub category: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct ToolEntry {
    pub(crate) name: &'static str,
    description: &'static str,
}

pub(crate) fn catalog() -> BTreeMap<&'static str, Vec<ToolEntry>> {
    let mut m: BTreeMap<&'static str, Vec<ToolEntry>> = BTreeMap::new();

    macro_rules! cat {
        ($key:expr, [ $( ($name:expr, $desc:expr) ),* $(,)? ]) => {
            m.insert($key, vec![ $( ToolEntry { name: $name, description: $desc } ),* ]);
        };
    }

    cat!("file", [
        ("read_directory_tree",       "gitignore-aware directory tree"),
        ("read_file_outline",         "auto-detect file type and return outline"),
        ("read_markdown_toc",         "headings TOC from a Markdown file"),
        ("read_markdown_section",     "extract sections by anchor"),
        ("read_code_skeleton",        "function/struct/class signatures only"),
        ("read_code_body",            "full body of skeleton items by ID (zoom: body/sketch/skeleton/auto)"),
        ("read_code_sketch",          "control-flow sketch by ID (between skeleton and body)"),
        ("read_code_deps",            "imports + imported_by dependency graph"),
        ("read_type_skeleton",        "TS/Go/Rust type definitions with fields"),
        ("read_call_graph",           "callers/callees; depth>=1 for cross-file"),
        ("read_token_map",            "largest files first by token count"),
        ("read_workspace_stats",      "language breakdown across workspace"),
        ("read_interface_conformance","types implementing interface/trait"),
        ("search_file",               "keyword/regex search with context lines"),
        ("semantic_search",           "natural language → relevant code bodies"),
        ("read_context_pack",         "one-call task context: ranked files+symbols+bodies within budget"),
        ("read_symbol_usages",        "all usages of a symbol across workspace"),
        ("read_json_yaml_keys",       "key structure of JSON/YAML/TOML"),
        ("read_json_yaml_value",      "value at a key path"),
        ("batch_read",                "multiple read ops in one call"),
    ]);
    cat!("write", [
        ("patch_symbol",  "replace a symbol's source by skeleton ID"),
        ("rename_symbol", "rename a symbol workspace-wide (write counterpart of read_symbol_usages)"),
        ("create_file",   "create a new file (opt-in: --enable-writes)"),
        ("delete_symbol", "delete a symbol by ID — pairs with read_dead_code (opt-in: --enable-writes)"),
        ("insert_symbol", "insert code at a structural location (opt-in: --enable-writes)"),
        ("apply_edits",   "atomic multi-file find/replace — pairs with batch_read (opt-in: --enable-writes)"),
        ("set_config_value", "set a JSON/YAML/TOML value by dot-path — pairs with read_json_yaml_value (opt-in: --enable-writes)"),
        ("manage_imports",   "add/remove import statements, dedup (opt-in: --enable-writes)"),
        ("format_code",      "run rustfmt/prettier/black/gofmt on a file (opt-in: --enable-writes)"),
    ]);
    cat!("git", [
        ("read_git_diff",        "compressed diff vs HEAD or any ref"),
        ("read_git_log",         "structured commit log with filters"),
        ("read_git_blame_body",  "line-level blame for a function"),
        ("read_changed_files",   "files changed between branches"),
        ("read_git_stash",       "stash list and diff"),
        ("read_code_ownership",  "churn + per-author line share + last-modified (git log+blame)"),
    ]);
    cat!("schema", [
        ("read_db_schema",        "Prisma/SQL table list"),
        ("read_db_table",         "field definitions for a table"),
        ("read_css_skeleton",     "CSS/SCSS selector list"),
        ("read_css_body",         "ruleset body by selector ID"),
        ("read_graphql_schema",   "GraphQL type list"),
        ("read_graphql_type",     "fields of a GraphQL type"),
        ("read_proto_schema",     "Protobuf message/service list"),
        ("read_proto_type",       "fields of a proto message/service"),
        ("read_openapi",          "OpenAPI/Swagger endpoint list"),
        ("read_env_schema",       ".env.example / docker-compose variables"),
        ("read_package_manifest", "npm/cargo/go/python/maven/gradle unified"),
        ("read_ci_pipeline",      "GitHub Actions/GitLab/CircleCI structure"),
    ]);
    cat!("web", [
        ("fetch_webpage",         "HTML→MD with TOC"),
        ("read_webpage_section",  "section from cached webpage by anchor"),
        ("convert_document",      "PDF/DOCX → MD"),
    ]);
    cat!("notebook", [
        ("read_notebook_cells", "Jupyter cell list with output summaries"),
        ("read_notebook_cell",  "full cell content and output"),
    ]);
    cat!("test", [
        ("read_test_skeleton", "test suite structure (Jest/pytest/Cargo/Go)"),
        ("read_test_results",  "parse test runner output to summary"),
        ("read_test_coverage", "map coverage (lcov/coverage.py/cobertura) onto symbols"),
    ]);
    cat!("log", [
        ("read_log_tail",    "log file tail with level/pattern filter"),
        ("read_stack_trace", "stack trace → source context resolution"),
    ]);
    cat!("text", [
        ("compress_text",          "noise removal and compression"),
        ("count_tokens",           "token count for any text"),
        ("check_budget",           "token budget status and strategy"),
        ("delta_reset",            "reset delta-read ledger (full content on next read)"),
        ("summarize_conversation", "summarize conversation history"),
    ]);
    cat!("memory", [
        ("memory_save",   "persist key-value pair"),
        ("memory_get",    "retrieve by key"),
        ("memory_list",   "list by tag or keyword"),
        ("memory_delete", "delete by key"),
    ]);
    cat!("task", [
        ("task_create", "create a tracked task"),
        ("task_get",    "get task by ID"),
        ("task_update", "update task status/fields"),
        ("task_list",   "list tasks with filters"),
        ("task_delete", "delete task by ID"),
    ]);
    cat!("session", [
        ("session_snapshot", "save current work state"),
        ("session_restore",  "restore a snapshot"),
        ("session_list",     "list all snapshots"),
    ]);
    cat!("analysis", [
        ("read_complexity_map",  "cyclomatic complexity per function"),
        ("read_dead_code",       "unused symbols across workspace"),
        ("read_refactor_impact", "blast radius: callers + tests"),
        ("read_security_surface","injection/XSS/secrets/unsafe patterns"),
        ("read_dependency_audit","dependency vulnerability scan (cargo/npm/pip/osv audit)"),
        ("read_api_surface",     "public API surface (pub/export/__all__/Go caps)"),
        ("diff_schemas",         "schema diff between git refs (OpenAPI/Prisma/TS)"),
        ("read_pr_context",      "full PR context: files+skeletons+tests+commits"),
        ("read_type_diagnostics","LSP-equivalent type errors (cargo check/tsc/pyright/go vet) — opt-in: --enable-diagnostics"),
        ("project_digest",       "cached warm-start architecture summary (HEAD-invalidated)"),
    ]);
    cat!("cmd", [
        ("run_command", "run shell command with smart output filtering"),
    ]);
    cat!("debug", [
        ("debug_info", "server diagnostics: version, DB, tool count"),
        ("help",       "list tools by category (this tool)"),
    ]);

    m
}

pub fn help(params: HelpParams) -> serde_json::Value {
    let all = catalog();
    match params.category.as_deref() {
        None => {
            let mut categories: Vec<&str> = all.keys().copied().collect();
            categories.sort_unstable();
            serde_json::json!({
                "categories": categories,
                "usage": "help(category) → tool names + descriptions for that category; help(\"all\") → full catalog",
            })
        }
        Some("all") => serde_json::to_value(all).unwrap_or_default(),
        Some(cat) => {
            let key = cat.to_lowercase();
            if let Some(tools) = all.get(key.as_str()) {
                serde_json::json!({ &key: tools })
            } else {
                let mut categories: Vec<&str> = all.keys().copied().collect();
                categories.sort_unstable();
                serde_json::json!({
                    "error": format!("unknown category '{cat}'"),
                    "available_categories": categories,
                })
            }
        }
    }
}
