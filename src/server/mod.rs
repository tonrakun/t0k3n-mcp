use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rmcp::{
    ErrorData as McpError,
    ServerHandler,
    handler::server::tool::{Parameters, ToolRouter},
    model::*,
    tool, tool_handler, tool_router,
};
use serde::Serialize;

use crate::dashboard::DashboardState;

mod db;
pub mod tools;

use db::Database;
use tools::render::OutputFormat;
use tools::{
    batch::{BatchReadParams, batch_read},
    ci::{ReadCiPipelineParams, read_ci_pipeline},
    cmd::{CmdLedger, RunCommandParams, run_command},
    code::{ReadCallGraphParams, ReadCodeBodyParams, ReadCodeSkeletonParams, ReadInterfaceConformanceParams, ReadSymbolUsagesParams, ReadTypeSkeletonParams, read_call_graph, read_code_body, read_code_skeleton, read_interface_conformance, read_symbol_usages, read_type_skeleton},
    complexity::{ReadComplexityMapParams, read_complexity_map},
    context_pack::{ReadContextPackParams, read_context_pack},
    dead_code::{ReadDeadCodeParams, read_dead_code},
    diff_schemas::{DiffSchemasParams, diff_schemas},
    impact::{ReadRefactorImpactParams, read_refactor_impact},
    pr_context::{ReadPrContextParams, read_pr_context},
    security_surface::{ReadSecuritySurfaceParams, read_security_surface},
    css::{ReadCssBodyParams, ReadCssSkeletonParams, read_css_body, read_css_skeleton},
    db_schema::{ReadDbSchemaParams, ReadDbTableParams, read_db_schema, read_db_table},
    delta::{Delta, DeltaResetParams, ReadLedger},
    deps::{ReadCodeDepsParams, read_code_deps},
    document::{ConvertDocumentParams, convert_document},
    env::{ReadEnvSchemaParams, read_env_schema},
    fs::{ReadDirectoryTreeParams, ReadTokenMapParams, SearchFileParams, read_directory_tree, read_token_map, search_file},
    git::{ReadChangedFilesParams, ReadGitBlameBodyParams, ReadGitDiffParams, ReadGitLogParams, ReadGitStashParams, read_changed_files, read_git_blame_body, read_git_diff, read_git_log, read_git_stash},
    graphql::{ReadGraphqlSchemaParams, ReadGraphqlTypeParams, read_graphql_schema, read_graphql_type},
    help::{HelpParams, help},
    log::{ReadLogTailParams, ReadStackTraceParams, read_log_tail, read_stack_trace},
    manifest::{ReadPackageManifestParams, read_package_manifest},
    openapi::{ReadOpenApiParams, read_openapi},
    stats::{ReadWorkspaceStatsParams, read_workspace_stats},
    json_yaml::{ReadJsonYamlKeysParams, ReadJsonYamlValueParams, read_json_yaml_keys, read_json_yaml_value},
    markdown::{ReadMarkdownSectionParams, ReadMarkdownTocParams, read_markdown_section, read_markdown_toc},
    memory::{MemoryDeleteParams, MemoryGetParams, MemoryListParams, MemorySaveParams, memory_delete, memory_get, memory_list, memory_save},
    notebook::{ReadNotebookCellParams, ReadNotebookCellsParams, read_notebook_cell, read_notebook_cells},
    outline::{ReadFileOutlineParams, read_file_outline},
    patch::{PatchSymbolParams, patch_symbol},
    proto::{ReadProtoSchemaParams, ReadProtoTypeParams, read_proto_schema, read_proto_type},
    search::{SemanticSearchParams, semantic_search},
    session::{SessionListParams, SessionRestoreParams, SessionSnapshotParams, session_list, session_restore, session_snapshot},
    task::{TaskCreateParams, TaskDeleteParams, TaskGetParams, TaskListParams, TaskUpdateParams, task_create, task_delete, task_get, task_list, task_update},
    test_results::{ReadTestResultsParams, read_test_results},
    test_skeleton::{ReadTestSkeletonParams, read_test_skeleton},
    text::{CheckBudgetParams, CompressTextParams, CountTokensParams, SummarizeConversationParams, check_budget, compress_text, count_tokens, summarize_conversation},
    web::{FetchWebpageParams, ReadWebpageSectionParams, fetch_webpage, read_webpage_section},
};

pub const REGISTERED_TOOLS: &[&str] = &[
    // File reading
    "read_directory_tree",
    "read_markdown_toc",
    "read_markdown_section",
    "search_file",
    "read_json_yaml_keys",
    "read_json_yaml_value",
    "read_code_skeleton",
    "read_code_body",
    "patch_symbol",
    "read_code_deps",
    "read_file_outline",
    "semantic_search",
    "read_context_pack",
    "read_symbol_usages",
    "read_type_skeleton",
    "read_call_graph",
    "read_token_map",
    // Git
    "read_git_diff",
    "read_git_log",
    "read_git_blame_body",
    "read_changed_files",
    "read_git_stash",
    // Schema / DSL
    "read_db_schema",
    "read_db_table",
    "read_css_skeleton",
    "read_css_body",
    "read_graphql_schema",
    "read_graphql_type",
    "read_proto_schema",
    "read_proto_type",
    "read_openapi",
    "read_env_schema",
    "read_package_manifest",
    "read_ci_pipeline",
    "read_workspace_stats",
    "read_interface_conformance",
    "batch_read",
    // Notebook
    "read_notebook_cells",
    "read_notebook_cell",
    // Test
    "read_test_skeleton",
    "read_test_results",
    // Log / Debug
    "read_log_tail",
    "read_stack_trace",
    // Web / Document
    "fetch_webpage",
    "read_webpage_section",
    "convert_document",
    // Text / Budget
    "compress_text",
    "count_tokens",
    "check_budget",
    "summarize_conversation",
    // Memory / Task / Session
    "memory_save",
    "memory_get",
    "memory_list",
    "memory_delete",
    "task_create",
    "task_get",
    "task_update",
    "task_list",
    "task_delete",
    "session_snapshot",
    "session_restore",
    "session_list",
    "debug_info",
    "help",
    "delta_reset",
    // Phase 6 — Command execution
    "run_command",
    // Phase 5 — Differentiating analysis
    "read_complexity_map",
    "read_dead_code",
    "read_refactor_impact",
    "read_security_surface",
    "diff_schemas",
    "read_pr_context",
];

#[derive(Clone)]
pub struct T0k3nServer {
    pub root: PathBuf,
    db: Arc<Mutex<Database>>,
    web_cache: Arc<Mutex<HashMap<String, String>>>,
    ledger: Arc<Mutex<ReadLedger>>,
    cmd_ledger: Arc<Mutex<CmdLedger>>,
    tool_router: ToolRouter<Self>,
    pub dashboard: Option<Arc<DashboardState>>,
}

fn err(msg: impl std::fmt::Display) -> McpError {
    McpError::internal_error(msg.to_string(), None)
}

static OUTPUT_FORMAT: std::sync::OnceLock<OutputFormat> = std::sync::OnceLock::new();

/// Set once at startup (before serving). Defaults to Compact when unset.
pub fn set_output_format(format: OutputFormat) {
    let _ = OUTPUT_FORMAT.set(format);
}

fn output_format() -> OutputFormat {
    *OUTPUT_FORMAT.get().unwrap_or(&OutputFormat::Compact)
}

fn ok_json<T: Serialize>(v: T) -> Result<CallToolResult, McpError> {
    let s = match output_format() {
        OutputFormat::Json => serde_json::to_string_pretty(&v).map_err(err)?,
        OutputFormat::Compact => {
            let value = serde_json::to_value(&v).map_err(err)?;
            tools::render::to_compact_text(&value)
        }
    };
    Ok(CallToolResult::success(vec![Content::text(s)]))
}

fn ok_text(s: String) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![Content::text(s)]))
}

/// Pull `token_count` out of a tool response rendered as JSON or compact text.
fn extract_token_count(text: &str) -> Option<u64> {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(text)
        && let Some(tc) = v.get("token_count").and_then(|tc| tc.as_u64()) {
            return Some(tc);
        }
    text.lines()
        .find_map(|l| l.trim().strip_prefix("token_count: ").and_then(|n| n.trim().parse().ok()))
}

/// Ledger key for delta reads: tool name + canonical params.
fn delta_key<P: Serialize>(tool: &str, params: &P) -> String {
    format!("{tool}:{}", serde_json::to_string(params).unwrap_or_default())
}

/// Wraps a tool body: captures timing, records to dashboard on completion.
/// The inner async block contains the `?` operators so early-exit errors are still recorded.
macro_rules! instrument {
    ($self:expr, $name:literal, $body:block) => {{
        let __t = Instant::now();
        let __r: Result<CallToolResult, McpError> = async $body.await;
        if let Some(ref __d) = $self.dashboard {
            let __d = __d.clone();
            let __ms = __t.elapsed().as_millis() as u64;
            let __ok = __r.is_ok();
            // Extract token_count from the response content (JSON or compact text)
            let __tok: Option<u64> = __r.as_ref().ok().and_then(|ctr| {
                ctr.content.first().and_then(|c| {
                    serde_json::to_string(c).ok()
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                        .and_then(|v| v.get("text").and_then(|t| t.as_str()).map(|s| s.to_string()))
                        .and_then(|text| extract_token_count(&text))
                })
            });
            tokio::spawn(async move { __d.record_call($name.to_string(), __ms, __ok, __tok).await; });
        }
        __r
    }};
}

#[tool_router(router = tool_router)]
impl T0k3nServer {
    pub fn new(root: String, dashboard: Option<Arc<DashboardState>>) -> Self {
        let root_path = PathBuf::from(&root);
        let db_path = root_path.join(".t0k3n").join("t0k3n.db");
        let db = Database::new(&db_path).unwrap_or_else(|e| {
            tracing::warn!("Failed to open DB at {:?}: {}. Using in-memory DB.", db_path, e);
            Database::new(std::path::Path::new(":memory:")).unwrap()
        });
        let server = Self {
            root: root_path,
            db: Arc::new(Mutex::new(db)),
            web_cache: Arc::new(Mutex::new(HashMap::new())),
            ledger: Arc::new(Mutex::new(ReadLedger::new())),
            cmd_ledger: Arc::new(Mutex::new(CmdLedger::new())),
            tool_router: Self::tool_router(),
            dashboard,
        };
        tracing::info!(
            "t0k3n-mcp v{} initialized — {} tools registered: {}",
            env!("CARGO_PKG_VERSION"),
            REGISTERED_TOOLS.len(),
            REGISTERED_TOOLS.join(", ")
        );
        server
    }

    /// Render a tool response, consulting the delta-read ledger first.
    /// Repeat reads of unchanged content return a tiny "unchanged" stub;
    /// changed content returns a unified diff when that is cheaper.
    fn ok_delta(&self, key: String, v: serde_json::Value) -> Result<CallToolResult, McpError> {
        let rendered = match output_format() {
            OutputFormat::Json => serde_json::to_string_pretty(&v).map_err(err)?,
            OutputFormat::Compact => tools::render::to_compact_text(&v),
        };
        let delta = self.ledger.lock().unwrap().check_and_update(&key, &rendered);
        match delta {
            Delta::Full => ok_text(rendered),
            Delta::Unchanged { full_tokens } => ok_json(serde_json::json!({
                "unchanged": true,
                "note": "Identical to what you already received earlier this session — content not re-sent. Call delta_reset (optionally with a path pattern) and retry if you need the full content again.",
                "full_token_count": full_tokens,
                "token_count": 50,
            })),
            Delta::Diff { diff, full_tokens } => {
                let token_count = tools::fs::estimate_tokens(&diff);
                ok_json(serde_json::json!({
                    "changed": true,
                    "diff": diff,
                    "note": "Unified diff against what you received earlier this session. Call delta_reset and retry for the full content.",
                    "full_token_count": full_tokens,
                    "token_count": token_count,
                }))
            }
        }
    }

    // ─────────────────────────────────────────────
    // File reading tools
    // ─────────────────────────────────────────────

    #[tool(description = "Get .gitignore-aware directory tree. Use to explore workspace structure before reading files.")]
    async fn read_directory_tree(
        &self,
        Parameters(params): Parameters<ReadDirectoryTreeParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_directory_tree", {
            let key = delta_key("read_directory_tree", &params);
            let result = read_directory_tree(&self.root, params).map_err(err)?;
            self.ok_delta(key, serde_json::json!({ "tree": result.tree, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Get all headings (TOC) from a Markdown file. Call before read_markdown_section to get anchors.")]
    async fn read_markdown_toc(
        &self,
        Parameters(params): Parameters<ReadMarkdownTocParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_markdown_toc", {
            let key = delta_key("read_markdown_toc", &params);
            let result = read_markdown_toc(&self.root, params).map_err(err)?;
            self.ok_delta(key, serde_json::json!({ "toc": result.toc, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Get specific sections from a Markdown file by anchor. Call read_markdown_toc first to get anchors.")]
    async fn read_markdown_section(
        &self,
        Parameters(params): Parameters<ReadMarkdownSectionParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_markdown_section", {
            let key = delta_key("read_markdown_section", &params);
            let result = read_markdown_section(&self.root, params).map_err(err)?;
            self.ok_delta(key, serde_json::json!({ "sections": result.sections, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Search a file for a keyword or regex pattern with surrounding context lines.")]
    async fn search_file(
        &self,
        Parameters(params): Parameters<SearchFileParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "search_file", {
            let result = search_file(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({ "matches": result.matches, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Get key structure of a JSON or YAML file. Call before read_json_yaml_value to identify key paths.")]
    async fn read_json_yaml_keys(
        &self,
        Parameters(params): Parameters<ReadJsonYamlKeysParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_json_yaml_keys", {
            let key = delta_key("read_json_yaml_keys", &params);
            let result = read_json_yaml_keys(&self.root, params).map_err(err)?;
            self.ok_delta(key, serde_json::json!({ "keys": result.keys, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Get a specific value from a JSON or YAML file by dot-notation key path (e.g. 'dependencies.tokio').")]
    async fn read_json_yaml_value(
        &self,
        Parameters(params): Parameters<ReadJsonYamlValueParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_json_yaml_value", {
            let key = delta_key("read_json_yaml_value", &params);
            let result = read_json_yaml_value(&self.root, params).map_err(err)?;
            self.ok_delta(key, serde_json::json!({ "value": result.value, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Get code skeleton (functions, structs, classes) with signatures only. Call before read_code_body.")]
    async fn read_code_skeleton(
        &self,
        Parameters(params): Parameters<ReadCodeSkeletonParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_code_skeleton", {
            let key = delta_key("read_code_skeleton", &params);
            let result = read_code_skeleton(&self.root, params).map_err(err)?;
            self.ok_delta(key, serde_json::json!({ "language": result.language, "skeleton": result.skeleton, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Get full body of specific code items by ID from read_code_skeleton.")]
    async fn read_code_body(
        &self,
        Parameters(params): Parameters<ReadCodeBodyParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_code_body", {
            let key = delta_key("read_code_body", &params);
            let result = read_code_body(&self.root, params).map_err(err)?;
            self.ok_delta(key, serde_json::json!({ "items": result.items, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Replace one symbol's source by skeleton ID — write counterpart of read_code_body. Flow: read_code_skeleton → read_code_body(id) → patch_symbol(id, new_body|edits). For small changes pass edits=[{find,replace}] instead of new_body — find only needs to be unique within the symbol, so unchanged lines are never resent. Pass expected_name to guard against stale line numbers; re-run read_code_skeleton after each successful patch before patching the same file again. dry_run previews the diff.")]
    async fn patch_symbol(
        &self,
        Parameters(params): Parameters<PatchSymbolParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "patch_symbol", {
            let result = patch_symbol(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "written": result.written,
                "new_id": result.new_id,
                "lines_before": result.lines_before,
                "lines_after": result.lines_after,
                "diff": result.diff,
                "token_count": tools::fs::estimate_tokens(&result.diff),
            }))
        })
    }

    #[tool(description = "Get import/dependency graph for a code file. Returns what it imports and what files import it (imported_by). direction: \"imports\" | \"imported_by\" | \"both\".")]
    async fn read_code_deps(
        &self,
        Parameters(params): Parameters<ReadCodeDepsParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_code_deps", {
            let result = read_code_deps(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path, "language": result.language,
                "imports": result.imports, "imported_by": result.imported_by,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Get a unified outline of any file. Auto-detects type: code → skeleton, markdown → TOC, json/yaml → keys. Single entry point — no need to know the file type first.")]
    async fn read_file_outline(
        &self,
        Parameters(params): Parameters<ReadFileOutlineParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_file_outline", {
            let key = delta_key("read_file_outline", &params);
            let result = read_file_outline(&self.root, params).map_err(err)?;
            self.ok_delta(key, serde_json::json!({
                "path": result.path, "kind": result.kind, "language": result.language,
                "outline": result.outline, "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "One-call task context collection: ranks workspace files and symbols by relevance to a task description, returns ranked files + relevant signatures + top symbol bodies, greedily filled up to a token budget. Replaces the tree→search→skeleton→body round-trip sequence when starting a task. No subprocess needed (lexical ranking).")]
    async fn read_context_pack(
        &self,
        Parameters(params): Parameters<ReadContextPackParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_context_pack", {
            let result = read_context_pack(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "keywords": result.keywords,
                "files": result.files,
                "symbols": result.symbols,
                "bodies": result.bodies,
                "bodies_omitted_for_budget": result.bodies_omitted_for_budget,
                "budget": result.budget,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Search code semantically using a natural language query. Spawns Claude CLI to identify relevant functions from the skeleton, then returns their bodies. Requires `claude` CLI to be installed and authenticated.")]
    async fn semantic_search(
        &self,
        Parameters(params): Parameters<SemanticSearchParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "semantic_search", {
            let result = semantic_search(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({ "items": result.items, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Get compressed git diff. Defaults to all uncommitted changes vs HEAD. Use stat_only for a quick file-level summary.")]
    async fn read_git_diff(
        &self,
        Parameters(params): Parameters<ReadGitDiffParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_git_diff", {
            let result = read_git_diff(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({ "diff": result.diff, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Get structured git commit log with sha, author, date, message, and changed files. Filter by path, author, date range, or limit.")]
    async fn read_git_log(
        &self,
        Parameters(params): Parameters<ReadGitLogParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_git_log", {
            let result = read_git_log(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({ "entries": result.entries, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Get per-line blame (author + date) for a specific line range in a file. Use start_line/end_line from read_code_skeleton to target a function body.")]
    async fn read_git_blame_body(
        &self,
        Parameters(params): Parameters<ReadGitBlameBodyParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_git_blame_body", {
            let result = read_git_blame_body(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({ "path": result.path, "lines": result.lines, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Find all usages of a symbol name (function, struct, class, variable) across the workspace. Returns file path, line number, and context for each match. Max 100 results.")]
    async fn read_symbol_usages(
        &self,
        Parameters(params): Parameters<ReadSymbolUsagesParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_symbol_usages", {
            let result = read_symbol_usages(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "symbol": result.symbol, "usages": result.usages,
                "total": result.total, "truncated": result.truncated,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Parse an OpenAPI / Swagger spec (JSON or YAML) and return a compact endpoint summary: method, path, operation_id, summary, parameters, request body, and responses.")]
    async fn read_openapi(
        &self,
        Parameters(params): Parameters<ReadOpenApiParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_openapi", {
            let result = read_openapi(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "title": result.title, "version": result.version,
                "base_url": result.base_url, "spec_version": result.spec_version,
                "endpoints": result.endpoints, "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Extract environment variable definitions from .env.example / .env.sample / .env.template / docker-compose.yml. Returns key, description (from comments), default value, and required flag. Omit path to auto-scan workspace root.")]
    async fn read_env_schema(
        &self,
        Parameters(params): Parameters<ReadEnvSchemaParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_env_schema", {
            let result = read_env_schema(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({ "vars": result.vars, "sources": result.sources, "token_count": result.token_count }))
        })
    }

    // ─────────────────────────────────────────────
    // Web tools
    // ─────────────────────────────────────────────

    #[tool(description = "Fetch a webpage, convert HTML to Markdown, return TOC only. Call read_webpage_section to read specific sections.")]
    async fn fetch_webpage(
        &self,
        Parameters(params): Parameters<FetchWebpageParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "fetch_webpage", {
            let cache = self.web_cache.clone();
            let result = fetch_webpage(params, cache).await.map_err(err)?;
            ok_json(serde_json::json!({ "toc": result.toc, "token_count": result.token_count, "cached": result.cached }))
        })
    }

    #[tool(description = "Get specific sections from a cached webpage by anchor. Call fetch_webpage first.")]
    async fn read_webpage_section(
        &self,
        Parameters(params): Parameters<ReadWebpageSectionParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_webpage_section", {
            let cache = self.web_cache.clone();
            let result = read_webpage_section(params, cache).map_err(err)?;
            ok_json(serde_json::json!({ "sections": result.sections, "token_count": result.token_count }))
        })
    }

    // ─────────────────────────────────────────────
    // Document conversion
    // ─────────────────────────────────────────────

    #[tool(description = "Convert a PDF or DOCX to Markdown, return TOC and tmp_path. Use read_markdown_section(tmp_path) to read sections.")]
    async fn convert_document(
        &self,
        Parameters(params): Parameters<ConvertDocumentParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "convert_document", {
            let result = convert_document(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({ "toc": result.toc, "tmp_path": result.tmp_path, "token_count": result.token_count }))
        })
    }

    // ─────────────────────────────────────────────
    // Text tools
    // ─────────────────────────────────────────────

    #[tool(description = "Compress text by removing excessive whitespace and noise. Returns compressed text with token stats.")]
    async fn compress_text(
        &self,
        Parameters(params): Parameters<CompressTextParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "compress_text", { ok_json(compress_text(params)) })
    }

    #[tool(description = "Count approximate tokens, characters, and lines in a text.")]
    async fn count_tokens(
        &self,
        Parameters(params): Parameters<CountTokensParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "count_tokens", { ok_json(count_tokens(params)) })
    }

    #[tool(description = "Check token budget and get reading strategy (normal/conservative/aggressive/critical).")]
    async fn check_budget(
        &self,
        Parameters(params): Parameters<CheckBudgetParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "check_budget", { ok_json(check_budget(params)) })
    }

    #[tool(description = "Summarize conversation text to fit within a token budget.")]
    async fn summarize_conversation(
        &self,
        Parameters(params): Parameters<SummarizeConversationParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "summarize_conversation", { ok_json(summarize_conversation(params)) })
    }

    // ─────────────────────────────────────────────
    // Memory tools
    // ─────────────────────────────────────────────

    #[tool(description = "Save a key-value memory to persistent storage (.t0k3n/t0k3n.db).")]
    async fn memory_save(
        &self,
        Parameters(params): Parameters<MemorySaveParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "memory_save", {
            let db = self.db.lock().unwrap();
            ok_text(memory_save(&db, params).map_err(err)?)
        })
    }

    #[tool(description = "Get a memory entry by key.")]
    async fn memory_get(
        &self,
        Parameters(params): Parameters<MemoryGetParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "memory_get", {
            let db = self.db.lock().unwrap();
            ok_json(memory_get(&db, params).map_err(err)?)
        })
    }

    #[tool(description = "List all memories, optionally filtered by tag or keyword search.")]
    async fn memory_list(
        &self,
        Parameters(params): Parameters<MemoryListParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "memory_list", {
            let db = self.db.lock().unwrap();
            let entries = memory_list(&db, params).map_err(err)?;
            let count = entries.len();
            ok_json(serde_json::json!({ "memories": entries, "count": count }))
        })
    }

    #[tool(description = "Delete a memory by key.")]
    async fn memory_delete(
        &self,
        Parameters(params): Parameters<MemoryDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "memory_delete", {
            let db = self.db.lock().unwrap();
            ok_text(memory_delete(&db, params).map_err(err)?)
        })
    }

    // ─────────────────────────────────────────────
    // Task tools
    // ─────────────────────────────────────────────

    #[tool(description = "Create a task with title, description, status (pending/in_progress/done/cancelled), priority, tags.")]
    async fn task_create(
        &self,
        Parameters(params): Parameters<TaskCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "task_create", {
            let db = self.db.lock().unwrap();
            ok_json(task_create(&db, params).map_err(err)?)
        })
    }

    #[tool(description = "Get a task by ID.")]
    async fn task_get(
        &self,
        Parameters(params): Parameters<TaskGetParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "task_get", {
            let db = self.db.lock().unwrap();
            ok_json(task_get(&db, params).map_err(err)?)
        })
    }

    #[tool(description = "Update a task's fields. Only provided fields are updated.")]
    async fn task_update(
        &self,
        Parameters(params): Parameters<TaskUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "task_update", {
            let db = self.db.lock().unwrap();
            ok_json(task_update(&db, params).map_err(err)?)
        })
    }

    #[tool(description = "List tasks, optionally filtered by status or tag.")]
    async fn task_list(
        &self,
        Parameters(params): Parameters<TaskListParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "task_list", {
            let db = self.db.lock().unwrap();
            let tasks = task_list(&db, params).map_err(err)?;
            let count = tasks.len();
            ok_json(serde_json::json!({ "tasks": tasks, "count": count }))
        })
    }

    #[tool(description = "Delete a task by ID.")]
    async fn task_delete(
        &self,
        Parameters(params): Parameters<TaskDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "task_delete", {
            let db = self.db.lock().unwrap();
            ok_text(task_delete(&db, params).map_err(err)?)
        })
    }

    // ─────────────────────────────────────────────
    // Session tools
    // ─────────────────────────────────────────────

    #[tool(description = "Save a snapshot of work state (arbitrary JSON) for restoration in future sessions.")]
    async fn session_snapshot(
        &self,
        Parameters(params): Parameters<SessionSnapshotParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "session_snapshot", {
            let db = self.db.lock().unwrap();
            ok_json(session_snapshot(&db, params).map_err(err)?)
        })
    }

    #[tool(description = "Restore a previously saved session snapshot by ID.")]
    async fn session_restore(
        &self,
        Parameters(params): Parameters<SessionRestoreParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "session_restore", {
            let db = self.db.lock().unwrap();
            ok_json(session_restore(&db, params).map_err(err)?)
        })
    }

    #[tool(description = "List saved session snapshots (most recent first).")]
    async fn session_list(
        &self,
        Parameters(params): Parameters<SessionListParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "session_list", {
            let db = self.db.lock().unwrap();
            let sessions = session_list(&db, params).map_err(err)?;
            let count = sessions.len();
            ok_json(serde_json::json!({ "sessions": sessions, "count": count }))
        })
    }

    // ─────────────────────────────────────────────
    // Schema / DSL tools
    // ─────────────────────────────────────────────

    #[tool(description = "Get table/model list from a Prisma or SQL schema file. Returns name, kind, and field count. Call read_db_table for field details of a specific table.")]
    async fn read_db_schema(
        &self,
        Parameters(params): Parameters<ReadDbSchemaParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_db_schema", {
            let result = read_db_schema(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path, "format": result.format,
                "tables": result.tables, "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Get full field definitions for a specific table or model from a Prisma or SQL schema. Call read_db_schema first to get the table list.")]
    async fn read_db_table(
        &self,
        Parameters(params): Parameters<ReadDbTableParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_db_table", {
            let result = read_db_table(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "name": result.name, "kind": result.kind,
                "fields": result.fields, "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Get CSS/SCSS/Less selector list with property counts. Returns IDs for use with read_css_body.")]
    async fn read_css_skeleton(
        &self,
        Parameters(params): Parameters<ReadCssSkeletonParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_css_skeleton", {
            let result = read_css_skeleton(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path, "selectors": result.selectors, "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Get full CSS rule content for specific selectors by ID. Call read_css_skeleton first to get selector IDs.")]
    async fn read_css_body(
        &self,
        Parameters(params): Parameters<ReadCssBodyParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_css_body", {
            let result = read_css_body(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({ "items": result.items, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Get type/input/enum/interface list from a GraphQL schema file. Returns IDs for use with read_graphql_type.")]
    async fn read_graphql_schema(
        &self,
        Parameters(params): Parameters<ReadGraphqlSchemaParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_graphql_schema", {
            let result = read_graphql_schema(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path, "types": result.types, "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Get full field definitions for a specific GraphQL type. Call read_graphql_schema first to get the type list.")]
    async fn read_graphql_type(
        &self,
        Parameters(params): Parameters<ReadGraphqlTypeParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_graphql_type", {
            let result = read_graphql_type(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "name": result.name, "kind": result.kind,
                "fields": result.fields, "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Get message/service/enum list from a .proto (Protocol Buffers) file. Returns IDs for use with read_proto_type.")]
    async fn read_proto_schema(
        &self,
        Parameters(params): Parameters<ReadProtoSchemaParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_proto_schema", {
            let result = read_proto_schema(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path, "syntax": result.syntax, "package": result.package,
                "types": result.types, "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Get full field definitions for a specific message, service, or enum in a .proto file. Call read_proto_schema first to get the type list.")]
    async fn read_proto_type(
        &self,
        Parameters(params): Parameters<ReadProtoTypeParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_proto_type", {
            let result = read_proto_type(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "name": result.name, "kind": result.kind,
                "fields": result.fields, "token_count": result.token_count,
            }))
        })
    }

    // ─────────────────────────────────────────────
    // Notebook tools
    // ─────────────────────────────────────────────

    #[tool(description = "Get cell list from a Jupyter notebook (.ipynb) with type, preview, and output count. Call before read_notebook_cell to choose which cells to read.")]
    async fn read_notebook_cells(
        &self,
        Parameters(params): Parameters<ReadNotebookCellsParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_notebook_cells", {
            let result = read_notebook_cells(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path, "nbformat": result.nbformat,
                "cells": result.cells, "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Get full source of a specific cell from a Jupyter notebook (.ipynb). Use the index from read_notebook_cells. Set include_outputs=true to also fetch cell outputs.")]
    async fn read_notebook_cell(
        &self,
        Parameters(params): Parameters<ReadNotebookCellParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_notebook_cell", {
            let result = read_notebook_cell(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path, "index": result.index, "cell_type": result.cell_type,
                "execution_count": result.execution_count, "source": result.source,
                "outputs": result.outputs, "token_count": result.token_count,
            }))
        })
    }

    // ─────────────────────────────────────────────
    // Log tools
    // ─────────────────────────────────────────────

    #[tool(description = "Read the tail of a log file with optional level (ERROR/WARN/INFO/DEBUG) and regex pattern filters. Returns last N lines and level counts across the whole file.")]
    async fn read_log_tail(
        &self,
        Parameters(params): Parameters<ReadLogTailParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_log_tail", {
            let result = read_log_tail(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path, "total_lines": result.total_lines,
                "returned_lines": result.returned_lines, "level_counts": result.level_counts,
                "lines": result.lines, "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Parse a stack trace and fetch source context around each referenced file:line. Supports Python, Rust, JavaScript/TypeScript, Java, Go, and C#. Returns resolved code snippets from workspace files.")]
    async fn read_stack_trace(
        &self,
        Parameters(params): Parameters<ReadStackTraceParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_stack_trace", {
            let result = read_stack_trace(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "total_frames": result.total_frames, "resolved_frames": result.resolved_frames,
                "frames": result.frames, "token_count": result.token_count,
            }))
        })
    }

    // ─────────────────────────────────────────────
    // Test tools
    // ─────────────────────────────────────────────

    #[tool(description = "Get test case list from a test file (Jest/pytest/Rust/#[test]/Go/JUnit/RSpec). Returns IDs usable with read_code_body to get test implementations.")]
    async fn read_test_skeleton(
        &self,
        Parameters(params): Parameters<ReadTestSkeletonParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_test_skeleton", {
            let result = read_test_skeleton(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path, "framework": result.framework,
                "tests": result.tests, "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Parse test runner output (Jest/Vitest/pytest/cargo test/go test) into a structured summary: pass/fail counts per suite and failure details. Accepts raw text or a file path.")]
    async fn read_test_results(
        &self,
        Parameters(params): Parameters<ReadTestResultsParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_test_results", {
            let result = read_test_results(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "framework": result.framework, "summary": result.summary,
                "suites": result.suites, "failures": result.failures,
                "token_count": result.token_count,
            }))
        })
    }

    // ─────────────────────────────────────────────
    // Extended code analysis tools
    // ─────────────────────────────────────────────

    #[tool(description = "Get type definitions (interface/type/enum/struct) with field names from TypeScript, Go, or Rust files. More detailed than read_code_skeleton for type-heavy files.")]
    async fn read_type_skeleton(
        &self,
        Parameters(params): Parameters<ReadTypeSkeletonParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_type_skeleton", {
            let key = delta_key("read_type_skeleton", &params);
            let result = read_type_skeleton(&self.root, params).map_err(err)?;
            self.ok_delta(key, serde_json::json!({
                "path": result.path, "language": result.language,
                "types": result.types, "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Get the call graph for a function: what functions it calls, and which functions in the same file call it. Uses function_id from read_code_skeleton.")]
    async fn read_call_graph(
        &self,
        Parameters(params): Parameters<ReadCallGraphParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_call_graph", {
            let result = read_call_graph(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "function": result.function, "file": result.file,
                "calls": result.calls, "called_by_in_file": result.called_by_in_file,
                "cross_file_callees": result.cross_file_callees,
                "cross_file_callers": result.cross_file_callers,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "List all workspace files sorted by estimated token count (largest first). Use to identify token-heavy files before reading. Supports glob filtering.")]
    async fn read_token_map(
        &self,
        Parameters(params): Parameters<ReadTokenMapParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_token_map", {
            let result = read_token_map(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "files": result.files, "total_tokens": result.total_tokens,
                "file_count": result.file_count, "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Get per-file change summary (added/deleted lines, status) for the current diff vs a base ref. Step 1 before read_git_diff — get the file list first, then read specific files' diffs.")]
    async fn read_changed_files(
        &self,
        Parameters(params): Parameters<ReadChangedFilesParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_changed_files", {
            let result = read_changed_files(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "base": result.base, "files": result.files,
                "total_added": result.total_added, "total_deleted": result.total_deleted,
                "file_count": result.file_count, "token_count": result.token_count,
            }))
        })
    }

    // ─────────────────────────────────────────────
    // Phase 4 tools
    // ─────────────────────────────────────────────

    #[tool(description = "List stashes and optionally get diff for a specific stash entry. Omit index to list only.")]
    async fn read_git_stash(
        &self,
        Parameters(params): Parameters<ReadGitStashParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_git_stash", {
            let result = read_git_stash(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "stashes": result.stashes, "diff": result.diff, "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Parse package.json / Cargo.toml / go.mod / pyproject.toml / pom.xml / build.gradle into a unified dependency list. Faster than read_json_yaml_value for multi-ecosystem projects.")]
    async fn read_package_manifest(
        &self,
        Parameters(params): Parameters<ReadPackageManifestParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_package_manifest", {
            let result = read_package_manifest(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "manifests": result.manifests, "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Parse CI pipeline configs (GitHub Actions / GitLab CI / CircleCI) into structured workflow/job/step summary. Omit path to auto-scan workspace.")]
    async fn read_ci_pipeline(
        &self,
        Parameters(params): Parameters<ReadCiPipelineParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_ci_pipeline", {
            let result = read_ci_pipeline(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "pipelines": result.pipelines, "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Get codebase-wide statistics: total files/lines/tokens, per-language breakdown with %, and top-10 largest files. Much faster overview than read_token_map.")]
    async fn read_workspace_stats(
        &self,
        Parameters(params): Parameters<ReadWorkspaceStatsParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_workspace_stats", {
            let result = read_workspace_stats(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "total_files": result.total_files, "total_lines": result.total_lines,
                "total_tokens": result.total_tokens, "by_language": result.by_language,
                "largest_files": result.largest_files, "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Find all types that implement a given interface/trait/abstract class across the workspace. Supports TypeScript, Rust, Java, Kotlin, Go, PHP, C#.")]
    async fn read_interface_conformance(
        &self,
        Parameters(params): Parameters<ReadInterfaceConformanceParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_interface_conformance", {
            let result = read_interface_conformance(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "name": result.name, "kind": result.kind,
                "implementations": result.implementations,
                "total": result.total, "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Execute multiple read operations in one call (code_skeleton | code_body | markdown_section | json_value | file_outline). Reduces round-trips when you need several files at once.")]
    async fn batch_read(
        &self,
        Parameters(params): Parameters<BatchReadParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "batch_read", {
            let result = batch_read(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "results": result.results, "total_token_count": result.total_token_count,
            }))
        })
    }

    // ─────────────────────────────────────────────
    // Phase 5 — Differentiating analysis tools
    // ─────────────────────────────────────────────

    #[tool(description = "Compute cyclomatic complexity for every function in a file or directory. Returns functions sorted by complexity with risk level (low/medium/high/critical). Use to identify refactoring targets without running a linter.")]
    async fn read_complexity_map(
        &self,
        Parameters(params): Parameters<ReadComplexityMapParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_complexity_map", {
            let result = read_complexity_map(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "entries": result.entries,
                "total_analyzed": result.total_analyzed,
                "high_risk_count": result.high_risk_count,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Find unused symbols (functions, classes, structs) that are defined but never called across the workspace. Works across all tree-sitter supported languages without a compiler or LSP.")]
    async fn read_dead_code(
        &self,
        Parameters(params): Parameters<ReadDeadCodeParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_dead_code", {
            let result = read_dead_code(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "entries": result.entries,
                "total_symbols_checked": result.total_symbols_checked,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Blast-radius analysis for a refactor: given a symbol name, returns all callers, all files that reference it, and all test files that cover it — in one call. Combines call_graph + symbol_usages + test discovery.")]
    async fn read_refactor_impact(
        &self,
        Parameters(params): Parameters<ReadRefactorImpactParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_refactor_impact", {
            let result = read_refactor_impact(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "symbol": result.symbol,
                "definition_file": result.definition_file,
                "definition_line": result.definition_line,
                "direct_callers": result.direct_callers,
                "direct_callees": result.direct_callees,
                "referenced_in": result.referenced_in,
                "total_references": result.total_references,
                "test_files": result.test_files,
                "blast_radius": result.blast_radius,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Static security surface scan: finds potential injection vectors, XSS sinks, hardcoded secrets, unsafe code, and path traversal patterns across the codebase. No compiler needed. Categories: injection, xss, secrets, unsafe, path_traversal, all.")]
    async fn read_security_surface(
        &self,
        Parameters(params): Parameters<ReadSecuritySurfaceParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_security_surface", {
            let result = read_security_surface(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "findings": result.findings,
                "total": result.total,
                "by_category": result.by_category,
                "by_severity": result.by_severity,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Diff a schema file (OpenAPI, Prisma/SQL, TypeScript types) between two git refs. Returns added/removed/modified endpoints, tables, or types. Auto-detects schema type from file extension.")]
    async fn diff_schemas(
        &self,
        Parameters(params): Parameters<DiffSchemasParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "diff_schemas", {
            let result = diff_schemas(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path,
                "schema_type": result.schema_type,
                "before_ref": result.before_ref,
                "after_ref": result.after_ref,
                "added": result.added,
                "removed": result.removed,
                "modified": result.modified,
                "total_changes": result.total_changes,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Load full PR context in one call: changed files with skeletons, diff stats, related test files, and commit list. Pass branch + base to get everything needed for a code review without multiple round-trips.")]
    async fn read_pr_context(
        &self,
        Parameters(params): Parameters<ReadPrContextParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_pr_context", {
            let result = read_pr_context(&self.root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "branch": result.branch,
                "base": result.base,
                "changed_files": result.changed_files,
                "total_files": result.total_files,
                "total_added": result.total_added,
                "total_deleted": result.total_deleted,
                "related_tests": result.related_tests,
                "commits": result.commits,
                "token_count": result.token_count,
            }))
        })
    }

    // ─────────────────────────────────────────────
    // Debug tool
    // ─────────────────────────────────────────────

    #[tool(description = "Execute a shell command and return token-efficient output. On success: last ~30 lines (final summary). On failure: extracted error lines + warning lines + last ~20 lines for context. Use for build tools (cargo, npm, go, make, mvn), test runners (cargo test, pytest, jest), linters (clippy, eslint, flake8), and type checkers (tsc, mypy). Repeat runs of the same command return only the delta: new/resolved/unchanged error and warning counts plus the new lines — unchanged lines equal what you already received. Call delta_reset and rerun for full output.")]
    async fn run_command(
        &self,
        Parameters(params): Parameters<RunCommandParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "run_command", {
            let key = CmdLedger::key(&params.command, params.cwd.as_deref());
            let result = run_command(&self.root, params).map_err(err)?;
            let delta = self.cmd_ledger.lock().unwrap().check_and_update(&key, &result);
            match delta {
                None => ok_json(serde_json::json!({
                    "command":     result.command,
                    "exit_code":   result.exit_code,
                    "success":     result.success,
                    "duration_ms": result.duration_ms,
                    "summary":     result.summary,
                    "errors":      result.errors,
                    "warnings":    result.warnings,
                    "token_count": result.token_count,
                })),
                Some(d) => {
                    let repr = format!(
                        "{}\n{}\n{}",
                        d.summary.as_deref().unwrap_or(""),
                        d.new_errors.join("\n"),
                        d.new_warnings.join("\n")
                    );
                    let mut v = serde_json::json!({
                        "command":     result.command,
                        "exit_code":   result.exit_code,
                        "success":     result.success,
                        "duration_ms": result.duration_ms,
                        "delta":       true,
                        "success_changed":   d.success_changed,
                        "errors_new":        d.new_errors,
                        "errors_resolved":   d.resolved_errors,
                        "errors_unchanged":  d.unchanged_errors,
                        "warnings_new":      d.new_warnings,
                        "warnings_resolved": d.resolved_warnings,
                        "warnings_unchanged": d.unchanged_warnings,
                        "note": "Delta vs the previous run of this command this session — unchanged errors/warnings not re-sent. Call delta_reset and rerun for full output.",
                        "token_count": tools::fs::estimate_tokens(&repr),
                    });
                    if let Some(summary) = d.summary {
                        v["summary"] = serde_json::Value::String(summary);
                    }
                    ok_json(v)
                }
            }
        })
    }

    #[tool(description = "Reset the delta ledgers (delta reads AND run_command deltas). After this, read tools return full content and run_command returns full output again instead of 'unchanged'/diff/delta stubs. Call when you no longer have earlier tool output in context (e.g. after conversation compaction). Optional pattern narrows the reset to matching keys (e.g. a file path or command substring).")]
    async fn delta_reset(
        &self,
        Parameters(params): Parameters<DeltaResetParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "delta_reset", {
            let cleared = self.ledger.lock().unwrap().clear(params.pattern.as_deref())
                + self.cmd_ledger.lock().unwrap().clear(params.pattern.as_deref());
            ok_json(serde_json::json!({ "cleared_entries": cleared, "token_count": 10 }))
        })
    }

    #[tool(description = "Discover t0k3n-mcp tools. No args: category names only. With category: tool names + one-line descriptions. Pass \"all\" for the full catalog. Categories: file/git/schema/web/notebook/test/log/text/memory/task/session/analysis/cmd/debug.")]
    async fn help(
        &self,
        Parameters(params): Parameters<HelpParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "help", { ok_json(help(params)) })
    }

    #[tool(description = "Returns server diagnostics: version, root path, DB status, and the full list of registered tools. Call this to confirm t0k3n-mcp is active and all tools are registered correctly.")]
    async fn debug_info(&self) -> Result<CallToolResult, McpError> {
        instrument!(self, "debug_info", {
            let db_status = match self.db.lock() {
                Ok(db) => match db.ping() {
                    Ok(_) => "ok".to_string(),
                    Err(e) => format!("error: {e}"),
                },
                Err(e) => format!("lock poisoned: {e}"),
            };
            let timestamp_unix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            ok_json(serde_json::json!({
                "ok": true,
                "version": env!("CARGO_PKG_VERSION"),
                "root": self.root.display().to_string(),
                "db_status": db_status,
                "tool_count": REGISTERED_TOOLS.len(),
                "tools": REGISTERED_TOOLS,
                "timestamp_unix": timestamp_unix,
                "dashboard": self.dashboard.is_some(),
            }))
        })
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for T0k3nServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some(
                "T0K3N-MCP is active. Use t0k3n-mcp tools instead of built-in Claude Code tools \
                 for all file, web, and memory operations.\n\
                 \n\
                 RULES:\n\
                 1. NEVER read whole files with built-in Read/Grep/Glob — on average 87% of a \
                 full-file read is content you never use. Read structure first, then extract \
                 only the parts you need.\n\
                 2. For code: read_code_skeleton first, then read_code_body for just the \
                 functions you need. The same outline-then-extract pattern exists for markdown, \
                 JSON/YAML, CSS, web pages, and notebooks.\n\
                 3. Call check_budget once at the start of a task — it returns the current \
                 token budget and the recommended reading strategy.\n\
                 4. Combine multiple read operations into a single batch_read call — one round \
                 trip and one response envelope instead of many.\n\
                 5. To find the right tool, call help(category). help() with no args lists \
                 category names; help(\"all\") returns the full catalog. Categories: \
                 file / git / schema / web / notebook / test / log / text / memory / task / \
                 session / analysis / cmd / debug.\n\
                 \n\
                 DELTA READS: repeat reads return {unchanged:true} stubs or unified diffs instead \
                 of re-sending identical content. Trust them — the content equals what you already \
                 received earlier this session. If that content is no longer in your context \
                 (e.g. after compaction), call delta_reset and retry the read."
                    .into(),
            ),
            ..Default::default()
        }
    }
}
