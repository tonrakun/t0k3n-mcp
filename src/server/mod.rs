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
use tools::{
    code::{ReadCallGraphParams, ReadCodeBodyParams, ReadCodeSkeletonParams, ReadSymbolUsagesParams, ReadTypeSkeletonParams, read_call_graph, read_code_body, read_code_skeleton, read_symbol_usages, read_type_skeleton},
    css::{ReadCssBodyParams, ReadCssSkeletonParams, read_css_body, read_css_skeleton},
    db_schema::{ReadDbSchemaParams, ReadDbTableParams, read_db_schema, read_db_table},
    deps::{ReadCodeDepsParams, read_code_deps},
    document::{ConvertDocumentParams, convert_document},
    env::{ReadEnvSchemaParams, read_env_schema},
    fs::{ReadDirectoryTreeParams, ReadTokenMapParams, SearchFileParams, read_directory_tree, read_token_map, search_file},
    git::{ReadChangedFilesParams, ReadGitBlameBodyParams, ReadGitDiffParams, ReadGitLogParams, read_changed_files, read_git_blame_body, read_git_diff, read_git_log},
    graphql::{ReadGraphqlSchemaParams, ReadGraphqlTypeParams, read_graphql_schema, read_graphql_type},
    openapi::{ReadOpenApiParams, read_openapi},
    json_yaml::{ReadJsonYamlKeysParams, ReadJsonYamlValueParams, read_json_yaml_keys, read_json_yaml_value},
    markdown::{ReadMarkdownSectionParams, ReadMarkdownTocParams, read_markdown_section, read_markdown_toc},
    memory::{MemoryDeleteParams, MemoryGetParams, MemoryListParams, MemorySaveParams, memory_delete, memory_get, memory_list, memory_save},
    outline::{ReadFileOutlineParams, read_file_outline},
    search::{SemanticSearchParams, semantic_search},
    session::{SessionListParams, SessionRestoreParams, SessionSnapshotParams, session_list, session_restore, session_snapshot},
    task::{TaskCreateParams, TaskDeleteParams, TaskGetParams, TaskListParams, TaskUpdateParams, task_create, task_delete, task_get, task_list, task_update},
    test_results::{ReadTestResultsParams, read_test_results},
    test_tools::{ReadTestSkeletonParams, read_test_skeleton},
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
    "read_code_deps",
    "read_file_outline",
    "semantic_search",
    "read_symbol_usages",
    "read_type_skeleton",
    "read_call_graph",
    "read_token_map",
    // Git
    "read_git_diff",
    "read_git_log",
    "read_git_blame_body",
    "read_changed_files",
    // Schema / DSL
    "read_db_schema",
    "read_db_table",
    "read_css_skeleton",
    "read_css_body",
    "read_graphql_schema",
    "read_graphql_type",
    "read_openapi",
    "read_env_schema",
    // Test
    "read_test_skeleton",
    "read_test_results",
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
];

#[derive(Clone)]
pub struct T0k3nServer {
    pub root: PathBuf,
    db: Arc<Mutex<Database>>,
    web_cache: Arc<Mutex<HashMap<String, String>>>,
    tool_router: ToolRouter<Self>,
    pub dashboard: Option<Arc<DashboardState>>,
}

fn err(msg: impl std::fmt::Display) -> McpError {
    McpError::internal_error(msg.to_string(), None)
}

fn ok_json<T: Serialize>(v: T) -> Result<CallToolResult, McpError> {
    let s = serde_json::to_string_pretty(&v).map_err(|e| err(e))?;
    Ok(CallToolResult::success(vec![Content::text(s)]))
}

fn ok_text(s: String) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![Content::text(s)]))
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
            tokio::spawn(async move { __d.record_call($name.to_string(), __ms, __ok).await; });
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

    // ─────────────────────────────────────────────
    // File reading tools
    // ─────────────────────────────────────────────

    #[tool(description = "Get .gitignore-aware directory tree. Use to explore workspace structure before reading files.")]
    async fn read_directory_tree(
        &self,
        Parameters(params): Parameters<ReadDirectoryTreeParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_directory_tree", {
            let result = read_directory_tree(&self.root, params).map_err(|e| err(e))?;
            ok_json(serde_json::json!({ "tree": result.tree, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Get all headings (TOC) from a Markdown file. Call before read_markdown_section to get anchors.")]
    async fn read_markdown_toc(
        &self,
        Parameters(params): Parameters<ReadMarkdownTocParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_markdown_toc", {
            let result = read_markdown_toc(&self.root, params).map_err(|e| err(e))?;
            ok_json(serde_json::json!({ "toc": result.toc, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Get specific sections from a Markdown file by anchor. Call read_markdown_toc first to get anchors.")]
    async fn read_markdown_section(
        &self,
        Parameters(params): Parameters<ReadMarkdownSectionParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_markdown_section", {
            let result = read_markdown_section(&self.root, params).map_err(|e| err(e))?;
            ok_json(serde_json::json!({ "sections": result.sections, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Search a file for a keyword or regex pattern with surrounding context lines.")]
    async fn search_file(
        &self,
        Parameters(params): Parameters<SearchFileParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "search_file", {
            let result = search_file(&self.root, params).map_err(|e| err(e))?;
            ok_json(serde_json::json!({ "matches": result.matches, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Get key structure of a JSON or YAML file. Call before read_json_yaml_value to identify key paths.")]
    async fn read_json_yaml_keys(
        &self,
        Parameters(params): Parameters<ReadJsonYamlKeysParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_json_yaml_keys", {
            let result = read_json_yaml_keys(&self.root, params).map_err(|e| err(e))?;
            ok_json(serde_json::json!({ "keys": result.keys, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Get a specific value from a JSON or YAML file by dot-notation key path (e.g. 'dependencies.tokio').")]
    async fn read_json_yaml_value(
        &self,
        Parameters(params): Parameters<ReadJsonYamlValueParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_json_yaml_value", {
            let result = read_json_yaml_value(&self.root, params).map_err(|e| err(e))?;
            ok_json(serde_json::json!({ "value": result.value, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Get code skeleton (functions, structs, classes) with signatures only. Call before read_code_body.")]
    async fn read_code_skeleton(
        &self,
        Parameters(params): Parameters<ReadCodeSkeletonParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_code_skeleton", {
            let result = read_code_skeleton(&self.root, params).map_err(|e| err(e))?;
            ok_json(serde_json::json!({ "language": result.language, "skeleton": result.skeleton, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Get full body of specific code items by ID from read_code_skeleton.")]
    async fn read_code_body(
        &self,
        Parameters(params): Parameters<ReadCodeBodyParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_code_body", {
            let result = read_code_body(&self.root, params).map_err(|e| err(e))?;
            ok_json(serde_json::json!({ "items": result.items, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Get import/dependency graph for a code file. Returns what it imports and what files import it (imported_by). direction: \"imports\" | \"imported_by\" | \"both\".")]
    async fn read_code_deps(
        &self,
        Parameters(params): Parameters<ReadCodeDepsParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_code_deps", {
            let result = read_code_deps(&self.root, params).map_err(|e| err(e))?;
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
            let result = read_file_outline(&self.root, params).map_err(|e| err(e))?;
            ok_json(serde_json::json!({
                "path": result.path, "kind": result.kind, "language": result.language,
                "outline": result.outline, "token_count": result.token_count,
            }))
        })
    }

    #[tool(description = "Search code semantically using a natural language query. Spawns Claude CLI to identify relevant functions from the skeleton, then returns their bodies. Requires `claude` CLI to be installed and authenticated.")]
    async fn semantic_search(
        &self,
        Parameters(params): Parameters<SemanticSearchParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "semantic_search", {
            let result = semantic_search(&self.root, params).map_err(|e| err(e))?;
            ok_json(serde_json::json!({ "items": result.items, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Get compressed git diff. Defaults to all uncommitted changes vs HEAD. Use stat_only for a quick file-level summary.")]
    async fn read_git_diff(
        &self,
        Parameters(params): Parameters<ReadGitDiffParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_git_diff", {
            let result = read_git_diff(&self.root, params).map_err(|e| err(e))?;
            ok_json(serde_json::json!({ "diff": result.diff, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Get structured git commit log with sha, author, date, message, and changed files. Filter by path, author, date range, or limit.")]
    async fn read_git_log(
        &self,
        Parameters(params): Parameters<ReadGitLogParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_git_log", {
            let result = read_git_log(&self.root, params).map_err(|e| err(e))?;
            ok_json(serde_json::json!({ "entries": result.entries, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Get per-line blame (author + date) for a specific line range in a file. Use start_line/end_line from read_code_skeleton to target a function body.")]
    async fn read_git_blame_body(
        &self,
        Parameters(params): Parameters<ReadGitBlameBodyParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_git_blame_body", {
            let result = read_git_blame_body(&self.root, params).map_err(|e| err(e))?;
            ok_json(serde_json::json!({ "path": result.path, "lines": result.lines, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Find all usages of a symbol name (function, struct, class, variable) across the workspace. Returns file path, line number, and context for each match. Max 100 results.")]
    async fn read_symbol_usages(
        &self,
        Parameters(params): Parameters<ReadSymbolUsagesParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_symbol_usages", {
            let result = read_symbol_usages(&self.root, params).map_err(|e| err(e))?;
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
            let result = read_openapi(&self.root, params).map_err(|e| err(e))?;
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
            let result = read_env_schema(&self.root, params).map_err(|e| err(e))?;
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
            let result = fetch_webpage(params, cache).await.map_err(|e| err(e))?;
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
            let result = read_webpage_section(params, cache).map_err(|e| err(e))?;
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
            let result = convert_document(&self.root, params).map_err(|e| err(e))?;
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
            ok_text(memory_save(&db, params).map_err(|e| err(e))?)
        })
    }

    #[tool(description = "Get a memory entry by key.")]
    async fn memory_get(
        &self,
        Parameters(params): Parameters<MemoryGetParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "memory_get", {
            let db = self.db.lock().unwrap();
            ok_json(memory_get(&db, params).map_err(|e| err(e))?)
        })
    }

    #[tool(description = "List all memories, optionally filtered by tag or keyword search.")]
    async fn memory_list(
        &self,
        Parameters(params): Parameters<MemoryListParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "memory_list", {
            let db = self.db.lock().unwrap();
            let entries = memory_list(&db, params).map_err(|e| err(e))?;
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
            ok_text(memory_delete(&db, params).map_err(|e| err(e))?)
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
            ok_json(task_create(&db, params).map_err(|e| err(e))?)
        })
    }

    #[tool(description = "Get a task by ID.")]
    async fn task_get(
        &self,
        Parameters(params): Parameters<TaskGetParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "task_get", {
            let db = self.db.lock().unwrap();
            ok_json(task_get(&db, params).map_err(|e| err(e))?)
        })
    }

    #[tool(description = "Update a task's fields. Only provided fields are updated.")]
    async fn task_update(
        &self,
        Parameters(params): Parameters<TaskUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "task_update", {
            let db = self.db.lock().unwrap();
            ok_json(task_update(&db, params).map_err(|e| err(e))?)
        })
    }

    #[tool(description = "List tasks, optionally filtered by status or tag.")]
    async fn task_list(
        &self,
        Parameters(params): Parameters<TaskListParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "task_list", {
            let db = self.db.lock().unwrap();
            let tasks = task_list(&db, params).map_err(|e| err(e))?;
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
            ok_text(task_delete(&db, params).map_err(|e| err(e))?)
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
            ok_json(session_snapshot(&db, params).map_err(|e| err(e))?)
        })
    }

    #[tool(description = "Restore a previously saved session snapshot by ID.")]
    async fn session_restore(
        &self,
        Parameters(params): Parameters<SessionRestoreParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "session_restore", {
            let db = self.db.lock().unwrap();
            ok_json(session_restore(&db, params).map_err(|e| err(e))?)
        })
    }

    #[tool(description = "List saved session snapshots (most recent first).")]
    async fn session_list(
        &self,
        Parameters(params): Parameters<SessionListParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "session_list", {
            let db = self.db.lock().unwrap();
            let sessions = session_list(&db, params).map_err(|e| err(e))?;
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
            let result = read_db_schema(&self.root, params).map_err(|e| err(e))?;
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
            let result = read_db_table(&self.root, params).map_err(|e| err(e))?;
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
            let result = read_css_skeleton(&self.root, params).map_err(|e| err(e))?;
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
            let result = read_css_body(&self.root, params).map_err(|e| err(e))?;
            ok_json(serde_json::json!({ "items": result.items, "token_count": result.token_count }))
        })
    }

    #[tool(description = "Get type/input/enum/interface list from a GraphQL schema file. Returns IDs for use with read_graphql_type.")]
    async fn read_graphql_schema(
        &self,
        Parameters(params): Parameters<ReadGraphqlSchemaParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_graphql_schema", {
            let result = read_graphql_schema(&self.root, params).map_err(|e| err(e))?;
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
            let result = read_graphql_type(&self.root, params).map_err(|e| err(e))?;
            ok_json(serde_json::json!({
                "name": result.name, "kind": result.kind,
                "fields": result.fields, "token_count": result.token_count,
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
            let result = read_test_skeleton(&self.root, params).map_err(|e| err(e))?;
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
            let result = read_test_results(&self.root, params).map_err(|e| err(e))?;
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
            let result = read_type_skeleton(&self.root, params).map_err(|e| err(e))?;
            ok_json(serde_json::json!({
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
            let result = read_call_graph(&self.root, params).map_err(|e| err(e))?;
            ok_json(serde_json::json!({
                "function": result.function, "file": result.file,
                "calls": result.calls, "called_by_in_file": result.called_by_in_file,
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
            let result = read_token_map(&self.root, params).map_err(|e| err(e))?;
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
            let result = read_changed_files(&self.root, params).map_err(|e| err(e))?;
            ok_json(serde_json::json!({
                "base": result.base, "files": result.files,
                "total_added": result.total_added, "total_deleted": result.total_deleted,
                "file_count": result.file_count, "token_count": result.token_count,
            }))
        })
    }

    // ─────────────────────────────────────────────
    // Debug tool
    // ─────────────────────────────────────────────

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
                "T0K3N-MCP is active. You MUST use t0k3n-mcp tools instead of built-in \
                 Claude Code tools for all file, web, and memory operations. \
                 Built-in tools (Read, Glob, Grep, WebFetch, etc.) waste 75-90% of tokens \
                 on content you do not need. T0K3N-MCP tools reduce token usage by up to 87%.\n\
                 \n\
                 MANDATORY SUBSTITUTIONS — never use the left column when t0k3n-mcp is active:\n\
                 - Read file        → read_code_skeleton + read_code_body  (code)\n\
                 - Read file        → read_markdown_toc + read_markdown_section  (markdown)\n\
                 - Read file        → read_json_yaml_keys + read_json_yaml_value  (json/yaml)\n\
                 - Glob / ls        → read_directory_tree\n\
                 - Grep / search    → search_file\n\
                 - WebFetch         → fetch_webpage + read_webpage_section\n\
                 - Memory files     → memory_save / memory_get / memory_list\n\
                 - Task tracking    → task_create / task_update / task_list\n\
                 \n\
                 WORKFLOW (always structure before content):\n\
                 - Directory: read_directory_tree\n\
                 - Markdown: read_markdown_toc → read_markdown_section\n\
                 - Code: read_code_skeleton → read_code_body\n\
                 - Any file: read_file_outline (auto-detects code/md/json/yaml)\n\
                 - Dependencies: read_code_deps (imports + imported_by)\n\
                 - Semantic: semantic_search (natural language → relevant code bodies)\n\
                 - Git: read_git_diff (compressed diff vs HEAD or any ref)\n\
                 - JSON/YAML: read_json_yaml_keys → read_json_yaml_value\n\
                 - Web: fetch_webpage → read_webpage_section\n\
                 - Docs: convert_document → read_markdown_section(tmp_path)\n\
                 - Budget: check_budget, compress_text, count_tokens\n\
                 - Memory: memory_save/get/list/delete\n\
                 - Tasks: task_create/update/get/list/delete\n\
                 - Sessions: session_snapshot/restore/list\n\
                 - DB schema: read_db_schema → read_db_table\n\
                 - CSS: read_css_skeleton → read_css_body\n\
                 - GraphQL: read_graphql_schema → read_graphql_type\n\
                 - Tests: read_test_skeleton (list), read_test_results (parse output)\n\
                 - Types: read_type_skeleton (TS/Go/Rust types with fields)\n\
                 - Calls: read_call_graph (callers/callees for a function)\n\
                 - Token map: read_token_map (largest files first)\n\
                 - Changed files: read_changed_files → read_git_diff (per-file)"
                    .into(),
            ),
            ..Default::default()
        }
    }
}
