use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rmcp::{
    ErrorData as McpError,
    ServerHandler,
    handler::server::tool::{Parameters, ToolRouter},
    model::*,
    tool, tool_handler, tool_router,
};
use serde::Serialize;

mod db;
pub mod tools;

use db::Database;
use tools::{
    code::{ReadCodeBodyParams, ReadCodeSkeletonParams, read_code_body, read_code_skeleton},
    document::{ConvertDocumentParams, convert_document},
    fs::{ReadDirectoryTreeParams, SearchFileParams, read_directory_tree, search_file},
    git::{ReadGitDiffParams, read_git_diff},
    json_yaml::{ReadJsonYamlKeysParams, ReadJsonYamlValueParams, read_json_yaml_keys, read_json_yaml_value},
    markdown::{ReadMarkdownSectionParams, ReadMarkdownTocParams, read_markdown_section, read_markdown_toc},
    memory::{MemoryDeleteParams, MemoryGetParams, MemoryListParams, MemorySaveParams, memory_delete, memory_get, memory_list, memory_save},
    search::{SemanticSearchParams, semantic_search},
    session::{SessionListParams, SessionRestoreParams, SessionSnapshotParams, session_list, session_restore, session_snapshot},
    task::{TaskCreateParams, TaskDeleteParams, TaskGetParams, TaskListParams, TaskUpdateParams, task_create, task_delete, task_get, task_list, task_update},
    text::{CheckBudgetParams, CompressTextParams, CountTokensParams, SummarizeConversationParams, check_budget, compress_text, count_tokens, summarize_conversation},
    web::{FetchWebpageParams, ReadWebpageSectionParams, fetch_webpage, read_webpage_section},
};

#[derive(Clone)]
pub struct T0k3nServer {
    pub root: PathBuf,
    db: Arc<Mutex<Database>>,
    web_cache: Arc<Mutex<HashMap<String, String>>>,
    tool_router: ToolRouter<Self>,
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

#[tool_router(router = tool_router)]
impl T0k3nServer {
    pub fn new(root: String) -> Self {
        let root_path = PathBuf::from(&root);
        let db_path = root_path.join(".t0k3n").join("t0k3n.db");
        let db = Database::new(&db_path).unwrap_or_else(|e| {
            tracing::warn!("Failed to open DB at {:?}: {}. Using in-memory DB.", db_path, e);
            Database::new(std::path::Path::new(":memory:")).unwrap()
        });
        Self {
            root: root_path,
            db: Arc::new(Mutex::new(db)),
            web_cache: Arc::new(Mutex::new(HashMap::new())),
            tool_router: Self::tool_router(),
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
        let result = read_directory_tree(&self.root, params).map_err(|e| err(e))?;
        ok_json(serde_json::json!({
            "tree": result.tree,
            "token_count": result.token_count,
        }))
    }

    #[tool(description = "Get all headings (TOC) from a Markdown file. Call before read_markdown_section to get anchors.")]
    async fn read_markdown_toc(
        &self,
        Parameters(params): Parameters<ReadMarkdownTocParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = read_markdown_toc(&self.root, params).map_err(|e| err(e))?;
        ok_json(serde_json::json!({
            "toc": result.toc,
            "token_count": result.token_count,
        }))
    }

    #[tool(description = "Get specific sections from a Markdown file by anchor. Call read_markdown_toc first to get anchors.")]
    async fn read_markdown_section(
        &self,
        Parameters(params): Parameters<ReadMarkdownSectionParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = read_markdown_section(&self.root, params).map_err(|e| err(e))?;
        ok_json(serde_json::json!({
            "sections": result.sections,
            "token_count": result.token_count,
        }))
    }

    #[tool(description = "Search a file for a keyword or regex pattern with surrounding context lines.")]
    async fn search_file(
        &self,
        Parameters(params): Parameters<SearchFileParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = search_file(&self.root, params).map_err(|e| err(e))?;
        ok_json(serde_json::json!({
            "matches": result.matches,
            "token_count": result.token_count,
        }))
    }

    #[tool(description = "Get key structure of a JSON or YAML file. Call before read_json_yaml_value to identify key paths.")]
    async fn read_json_yaml_keys(
        &self,
        Parameters(params): Parameters<ReadJsonYamlKeysParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = read_json_yaml_keys(&self.root, params).map_err(|e| err(e))?;
        ok_json(serde_json::json!({
            "keys": result.keys,
            "token_count": result.token_count,
        }))
    }

    #[tool(description = "Get a specific value from a JSON or YAML file by dot-notation key path (e.g. 'dependencies.tokio').")]
    async fn read_json_yaml_value(
        &self,
        Parameters(params): Parameters<ReadJsonYamlValueParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = read_json_yaml_value(&self.root, params).map_err(|e| err(e))?;
        ok_json(serde_json::json!({
            "value": result.value,
            "token_count": result.token_count,
        }))
    }

    #[tool(description = "Get code skeleton (functions, structs, classes) with signatures only. Call before read_code_body.")]
    async fn read_code_skeleton(
        &self,
        Parameters(params): Parameters<ReadCodeSkeletonParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = read_code_skeleton(&self.root, params).map_err(|e| err(e))?;
        ok_json(serde_json::json!({
            "skeleton": result.skeleton,
            "token_count": result.token_count,
        }))
    }

    #[tool(description = "Get full body of specific code items by ID from read_code_skeleton.")]
    async fn read_code_body(
        &self,
        Parameters(params): Parameters<ReadCodeBodyParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = read_code_body(&self.root, params).map_err(|e| err(e))?;
        ok_json(serde_json::json!({
            "items": result.items,
            "token_count": result.token_count,
        }))
    }

    #[tool(description = "Search code semantically using a natural language query. Spawns Claude CLI to identify relevant functions from the skeleton, then returns their bodies. Requires `claude` CLI to be installed and authenticated.")]
    async fn semantic_search(
        &self,
        Parameters(params): Parameters<SemanticSearchParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = semantic_search(&self.root, params).map_err(|e| err(e))?;
        ok_json(serde_json::json!({
            "items": result.items,
            "token_count": result.token_count,
        }))
    }

    #[tool(description = "Get compressed git diff. Defaults to all uncommitted changes vs HEAD. Use stat_only for a quick file-level summary.")]
    async fn read_git_diff(
        &self,
        Parameters(params): Parameters<ReadGitDiffParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = read_git_diff(&self.root, params).map_err(|e| err(e))?;
        ok_json(serde_json::json!({
            "diff": result.diff,
            "token_count": result.token_count,
        }))
    }

    // ─────────────────────────────────────────────
    // Web tools
    // ─────────────────────────────────────────────

    #[tool(description = "Fetch a webpage, convert HTML to Markdown, return TOC only. Call read_webpage_section to read specific sections.")]
    async fn fetch_webpage(
        &self,
        Parameters(params): Parameters<FetchWebpageParams>,
    ) -> Result<CallToolResult, McpError> {
        let cache = self.web_cache.clone();
        let result = fetch_webpage(params, cache).await.map_err(|e| err(e))?;
        ok_json(serde_json::json!({
            "toc": result.toc,
            "token_count": result.token_count,
            "cached": result.cached,
        }))
    }

    #[tool(description = "Get specific sections from a cached webpage by anchor. Call fetch_webpage first.")]
    async fn read_webpage_section(
        &self,
        Parameters(params): Parameters<ReadWebpageSectionParams>,
    ) -> Result<CallToolResult, McpError> {
        let cache = self.web_cache.clone();
        let result = read_webpage_section(params, cache).map_err(|e| err(e))?;
        ok_json(serde_json::json!({
            "sections": result.sections,
            "token_count": result.token_count,
        }))
    }

    // ─────────────────────────────────────────────
    // Document conversion
    // ─────────────────────────────────────────────

    #[tool(description = "Convert a PDF or DOCX to Markdown, return TOC and tmp_path. Use read_markdown_section(tmp_path) to read sections.")]
    async fn convert_document(
        &self,
        Parameters(params): Parameters<ConvertDocumentParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = convert_document(&self.root, params).map_err(|e| err(e))?;
        ok_json(serde_json::json!({
            "toc": result.toc,
            "tmp_path": result.tmp_path,
            "token_count": result.token_count,
        }))
    }

    // ─────────────────────────────────────────────
    // Text tools
    // ─────────────────────────────────────────────

    #[tool(description = "Compress text by removing excessive whitespace and noise. Returns compressed text with token stats.")]
    async fn compress_text(
        &self,
        Parameters(params): Parameters<CompressTextParams>,
    ) -> Result<CallToolResult, McpError> {
        ok_json(compress_text(params))
    }

    #[tool(description = "Count approximate tokens, characters, and lines in a text.")]
    async fn count_tokens(
        &self,
        Parameters(params): Parameters<CountTokensParams>,
    ) -> Result<CallToolResult, McpError> {
        ok_json(count_tokens(params))
    }

    #[tool(description = "Check token budget and get reading strategy (normal/conservative/aggressive/critical).")]
    async fn check_budget(
        &self,
        Parameters(params): Parameters<CheckBudgetParams>,
    ) -> Result<CallToolResult, McpError> {
        ok_json(check_budget(params))
    }

    #[tool(description = "Summarize conversation text to fit within a token budget.")]
    async fn summarize_conversation(
        &self,
        Parameters(params): Parameters<SummarizeConversationParams>,
    ) -> Result<CallToolResult, McpError> {
        ok_json(summarize_conversation(params))
    }

    // ─────────────────────────────────────────────
    // Memory tools
    // ─────────────────────────────────────────────

    #[tool(description = "Save a key-value memory to persistent storage (.t0k3n/t0k3n.db).")]
    async fn memory_save(
        &self,
        Parameters(params): Parameters<MemorySaveParams>,
    ) -> Result<CallToolResult, McpError> {
        let db = self.db.lock().unwrap();
        let msg = memory_save(&db, params).map_err(|e| err(e))?;
        ok_text(msg)
    }

    #[tool(description = "Get a memory entry by key.")]
    async fn memory_get(
        &self,
        Parameters(params): Parameters<MemoryGetParams>,
    ) -> Result<CallToolResult, McpError> {
        let db = self.db.lock().unwrap();
        let entry = memory_get(&db, params).map_err(|e| err(e))?;
        ok_json(entry)
    }

    #[tool(description = "List all memories, optionally filtered by tag or keyword search.")]
    async fn memory_list(
        &self,
        Parameters(params): Parameters<MemoryListParams>,
    ) -> Result<CallToolResult, McpError> {
        let db = self.db.lock().unwrap();
        let entries = memory_list(&db, params).map_err(|e| err(e))?;
        let count = entries.len();
        ok_json(serde_json::json!({
            "memories": entries,
            "count": count,
        }))
    }

    #[tool(description = "Delete a memory by key.")]
    async fn memory_delete(
        &self,
        Parameters(params): Parameters<MemoryDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        let db = self.db.lock().unwrap();
        let msg = memory_delete(&db, params).map_err(|e| err(e))?;
        ok_text(msg)
    }

    // ─────────────────────────────────────────────
    // Task tools
    // ─────────────────────────────────────────────

    #[tool(description = "Create a task with title, description, status (pending/in_progress/done/cancelled), priority, tags.")]
    async fn task_create(
        &self,
        Parameters(params): Parameters<TaskCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        let db = self.db.lock().unwrap();
        let task = task_create(&db, params).map_err(|e| err(e))?;
        ok_json(task)
    }

    #[tool(description = "Get a task by ID.")]
    async fn task_get(
        &self,
        Parameters(params): Parameters<TaskGetParams>,
    ) -> Result<CallToolResult, McpError> {
        let db = self.db.lock().unwrap();
        let task = task_get(&db, params).map_err(|e| err(e))?;
        ok_json(task)
    }

    #[tool(description = "Update a task's fields. Only provided fields are updated.")]
    async fn task_update(
        &self,
        Parameters(params): Parameters<TaskUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        let db = self.db.lock().unwrap();
        let task = task_update(&db, params).map_err(|e| err(e))?;
        ok_json(task)
    }

    #[tool(description = "List tasks, optionally filtered by status or tag.")]
    async fn task_list(
        &self,
        Parameters(params): Parameters<TaskListParams>,
    ) -> Result<CallToolResult, McpError> {
        let db = self.db.lock().unwrap();
        let tasks = task_list(&db, params).map_err(|e| err(e))?;
        let count = tasks.len();
        ok_json(serde_json::json!({
            "tasks": tasks,
            "count": count,
        }))
    }

    #[tool(description = "Delete a task by ID.")]
    async fn task_delete(
        &self,
        Parameters(params): Parameters<TaskDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        let db = self.db.lock().unwrap();
        let msg = task_delete(&db, params).map_err(|e| err(e))?;
        ok_text(msg)
    }

    // ─────────────────────────────────────────────
    // Session tools
    // ─────────────────────────────────────────────

    #[tool(description = "Save a snapshot of work state (arbitrary JSON) for restoration in future sessions.")]
    async fn session_snapshot(
        &self,
        Parameters(params): Parameters<SessionSnapshotParams>,
    ) -> Result<CallToolResult, McpError> {
        let db = self.db.lock().unwrap();
        let session = session_snapshot(&db, params).map_err(|e| err(e))?;
        ok_json(session)
    }

    #[tool(description = "Restore a previously saved session snapshot by ID.")]
    async fn session_restore(
        &self,
        Parameters(params): Parameters<SessionRestoreParams>,
    ) -> Result<CallToolResult, McpError> {
        let db = self.db.lock().unwrap();
        let session = session_restore(&db, params).map_err(|e| err(e))?;
        ok_json(session)
    }

    #[tool(description = "List saved session snapshots (most recent first).")]
    async fn session_list(
        &self,
        Parameters(params): Parameters<SessionListParams>,
    ) -> Result<CallToolResult, McpError> {
        let db = self.db.lock().unwrap();
        let sessions = session_list(&db, params).map_err(|e| err(e))?;
        let count = sessions.len();
        ok_json(serde_json::json!({
            "sessions": sessions,
            "count": count,
        }))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for T0k3nServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "T0K3N-MCP: Token-saving MCP server for AI coding tools.\n\
                 \n\
                 WORKFLOW (always structure before content):\n\
                 - Directory: read_directory_tree\n\
                 - Markdown: read_markdown_toc → read_markdown_section\n\
                 - Code: read_code_skeleton → read_code_body\n\
                 - Semantic: semantic_search (natural language → relevant code bodies)\n\
                 - Git: read_git_diff (compressed diff vs HEAD or any ref)\n\
                 - JSON/YAML: read_json_yaml_keys → read_json_yaml_value\n\
                 - Web: fetch_webpage → read_webpage_section\n\
                 - Docs: convert_document → read_markdown_section(tmp_path)\n\
                 - Budget: check_budget, compress_text, count_tokens\n\
                 - Memory: memory_save/get/list/delete\n\
                 - Tasks: task_create/update/get/list/delete\n\
                 - Sessions: session_snapshot/restore/list"
                    .into(),
            ),
            ..Default::default()
        }
    }
}
