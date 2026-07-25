use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::tool::{FromToolCallContextPart, Parameters, ToolCallContext, ToolRouter},
    model::*,
    service::{RequestContext, RoleServer},
    tool, tool_handler, tool_router,
};
use serde::Serialize;

use crate::dashboard::DashboardState;

mod db;
pub mod tools;

use db::Database;
use tools::render::OutputFormat;
use tools::{
    api_surface::{ReadApiSurfaceParams, read_api_surface},
    audit::{ReadDependencyAuditParams, read_dependency_audit},
    batch::{BatchReadParams, batch_read},
    checkpoint::{EditCheckpointParams, RollbackParams, edit_checkpoint, rollback},
    ci::{ReadCiPipelineParams, read_ci_pipeline},
    cmd::{CmdLedger, RunCommandParams, run_command},
    code::{
        ReadCallGraphParams, ReadCodeBodyParams, ReadCodeSkeletonParams,
        ReadInterfaceConformanceParams, ReadSymbolUsagesParams, ReadTypeSkeletonParams,
        read_call_graph, read_code_body, read_code_skeleton, read_interface_conformance,
        read_symbol_usages, read_type_skeleton,
    },
    complexity::{ReadComplexityMapParams, read_complexity_map},
    config_write::{SetConfigValueParams, set_config_value},
    context_pack::{ReadContextPackParams, read_context_pack},
    coverage::{ReadTestCoverageParams, read_test_coverage},
    css::{ReadCssBodyParams, ReadCssSkeletonParams, read_css_body, read_css_skeleton},
    db_schema::{ReadDbSchemaParams, ReadDbTableParams, read_db_schema, read_db_table},
    dead_code::{ReadDeadCodeParams, read_dead_code},
    delta::{ContentDedup, ContentLedger, Delta, DeltaResetParams, ReadLedger},
    deps::{ReadCodeDepsParams, read_code_deps},
    diagnostics::{ReadTypeDiagnosticsParams, read_type_diagnostics},
    diff_schemas::{DiffSchemasParams, diff_schemas},
    digest::{ProjectDigestParams, project_digest},
    document::{ConvertDocumentParams, convert_document},
    env::{ReadEnvSchemaParams, read_env_schema},
    format::{FormatCodeParams, format_code},
    fs::{
        ReadDirectoryTreeParams, ReadTokenMapParams, SearchFileParams, read_directory_tree,
        read_token_map, search_file,
    },
    git::{
        ReadChangedFilesParams, ReadGitBlameBodyParams, ReadGitDiffParams, ReadGitLogParams,
        ReadGitStashParams, read_changed_files, read_git_blame_body, read_git_diff, read_git_log,
        read_git_stash,
    },
    graphql::{
        ReadGraphqlSchemaParams, ReadGraphqlTypeParams, read_graphql_schema, read_graphql_type,
    },
    help::{HelpParams, help},
    impact::{ReadRefactorImpactParams, read_refactor_impact},
    imports::{ManageImportsParams, manage_imports},
    json_yaml::{
        ReadJsonYamlKeysParams, ReadJsonYamlValueParams, read_json_yaml_keys, read_json_yaml_value,
    },
    log::{ReadLogTailParams, ReadStackTraceParams, read_log_tail, read_stack_trace},
    manifest::{ReadPackageManifestParams, read_package_manifest},
    markdown::{
        ReadMarkdownSectionParams, ReadMarkdownTocParams, read_markdown_section, read_markdown_toc,
    },
    markdown_write::{WriteMarkdownSectionParams, write_markdown_section},
    memory::{
        MemoryDeleteParams, MemoryGetParams, MemoryListParams, MemorySaveParams, memory_delete,
        memory_get, memory_list, memory_save,
    },
    move_symbol::{MoveSymbolParams, move_symbol},
    notebook::{
        ReadNotebookCellParams, ReadNotebookCellsParams, read_notebook_cell, read_notebook_cells,
    },
    openapi::{ReadOpenApiParams, read_openapi},
    outline::{ReadFileOutlineParams, read_file_outline},
    ownership::{ReadCodeOwnershipParams, read_code_ownership},
    patch::{PatchSymbolParams, patch_symbol},
    pr_context::{ReadPrContextParams, read_pr_context},
    proto::{ReadProtoSchemaParams, ReadProtoTypeParams, read_proto_schema, read_proto_type},
    rename::{RenameSymbolParams, rename_symbol},
    search::{SemanticSearchParams, semantic_search},
    security_surface::{ReadSecuritySurfaceParams, read_security_surface},
    session::{
        SessionListParams, SessionRestoreParams, SessionSnapshotParams, session_list,
        session_restore, session_snapshot,
    },
    sketch::{ReadCodeSketchParams, read_code_sketch},
    stats::{ReadWorkspaceStatsParams, read_workspace_stats},
    task::{
        TaskCreateParams, TaskDeleteParams, TaskGetParams, TaskListParams, TaskUpdateParams,
        task_create, task_delete, task_get, task_list, task_update,
    },
    test_results::{ReadTestResultsParams, read_test_results},
    test_skeleton::{ReadTestSkeletonParams, read_test_skeleton},
    text::{
        CheckBudgetParams, CompressTextParams, CountTokensParams, SummarizeConversationParams,
        check_budget, compress_text, count_tokens, summarize_conversation,
    },
    web::{FetchWebpageParams, ReadWebpageSectionParams, fetch_webpage, read_webpage_section},
    writes::{
        ApplyEditsParams, CreateFileParams, DeleteSymbolParams, InsertSymbolParams, apply_edits,
        create_file, delete_symbol, insert_symbol,
    },
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
    "read_code_sketch",
    "patch_symbol",
    "rename_symbol",
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
    "read_code_ownership",
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
    "read_test_coverage",
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
    "read_dependency_audit",
    "read_api_surface",
    "diff_schemas",
    "read_pr_context",
    // Phase 12 — LSP / type diagnostics
    "read_type_diagnostics",
    // Phase 11 — gen3 token reduction
    "project_digest",
    // Phase 14 — opt-in write tools (registered only with --enable-writes)
    "create_file",
    "delete_symbol",
    "insert_symbol",
    "apply_edits",
    // Phase 15 — more opt-in write tools
    "set_config_value",
    "manage_imports",
    "format_code",
    "move_symbol",
    "edit_checkpoint",
    "rollback",
    // Phase 18 — Markdown structural write
    "write_markdown_section",
];

/// Mutating write tools gated behind --enable-writes / T0K3N_ENABLE_WRITES.
/// Removed from the router unless writes are explicitly enabled. (patch_symbol
/// and rename_symbol predate the gate and stay always-on for compatibility.)
pub const WRITE_TOOLS: &[&str] = &[
    "create_file",
    "delete_symbol",
    "insert_symbol",
    "apply_edits",
    "set_config_value",
    "manage_imports",
    "format_code",
    "move_symbol",
    "edit_checkpoint",
    "rollback",
    "write_markdown_section",
];

/// Tools that execute arbitrary shell commands. Enabled by default (they predate
/// the capability model and are core to the build/test loop), but removable with
/// `--disable-commands` for setups that must not hand the agent a shell.
///
/// Note: while `run_command` is registered, the server is *not* read-only —
/// anything reachable from a shell is reachable. `--enable-writes` gates the
/// structured write tools, not the machine's writability.
pub const COMMAND_TOOLS: &[&str] = &["run_command"];

/// Tools that stay registered under every category profile: without them the agent
/// cannot discover what else it has or report its own configuration.
const ALWAYS_KEEP_TOOLS: &[&str] = &["help", "debug_info"];

/// Resolve a list of help() category names to the set of tool names to keep.
/// Unknown categories are ignored (validated and reported at startup instead).
fn tools_in_categories(categories: &[String]) -> std::collections::HashSet<&'static str> {
    let catalog = tools::help::catalog();
    let mut keep: std::collections::HashSet<&'static str> =
        ALWAYS_KEEP_TOOLS.iter().copied().collect();
    for cat in categories {
        if let Some(entries) = catalog.get(cat.trim().to_ascii_lowercase().as_str()) {
            keep.extend(entries.iter().map(|e| e.name));
        }
    }
    keep
}

/// Category names accepted by `--tools`, taken from the help() catalog so the two
/// can never drift apart.
pub fn known_tool_categories() -> Vec<&'static str> {
    tools::help::catalog().keys().copied().collect()
}

/// Startup capability configuration. Grouped into a struct so adding a capability
/// does not change every `T0k3nServer::new` call site.
#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub root: String,
    /// True when --root / T0K3N_ROOT was explicitly given.
    pub root_configured: bool,
    /// Register `read_type_diagnostics` (opt-in; spawns cargo check / tsc / …).
    pub diagnostics_enabled: bool,
    /// Register the structured write tools (opt-in).
    pub writes_enabled: bool,
    /// Register `run_command` (opt-out; on by default).
    pub commands_enabled: bool,
    /// When set, only tools in these help() categories are registered. Trims the
    /// tool-schema payload the client carries in every request.
    pub tool_categories: Option<Vec<String>>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            root: ".".to_string(),
            root_configured: false,
            diagnostics_enabled: false,
            writes_enabled: false,
            commands_enabled: true,
            tool_categories: None,
        }
    }
}

#[derive(Clone)]
pub struct T0k3nServer {
    pub root: PathBuf,
    db: Arc<Mutex<Database>>,
    web_cache: Arc<Mutex<HashMap<String, String>>>,
    ledger: Arc<Mutex<ReadLedger>>,
    cmd_ledger: Arc<Mutex<CmdLedger>>,
    content_ledger: Arc<Mutex<ContentLedger>>,
    /// Latest check_budget strategy (normal/conservative/aggressive/critical),
    /// used by read_code_body's zoom:auto to pick a detail level.
    budget_status: Arc<Mutex<Option<String>>>,
    tool_router: ToolRouter<Self>,
    config: ServerConfig,
    pub dashboard: Option<Arc<DashboardState>>,
}

fn err(msg: impl std::fmt::Display) -> McpError {
    McpError::internal_error(msg.to_string(), None)
}

/// Short content digest published with delta stubs so a caller can verify it still
/// holds the content the stub refers to. 12 hex chars is ample for that check and
/// costs a handful of tokens.
fn short_sha256(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())[..12].to_string()
}

/// Per-call extractor resolving the workspace root for a tool call. When the server
/// was started with an explicit root (--root / T0K3N_ROOT), the configured root always
/// wins. Otherwise, a `root` argument (absolute path) on the call overrides the
/// process's working-directory default — letting the server be used without any
/// MCP-side root configuration by passing it on every call instead.
struct EffectiveRoot(PathBuf);

/// Pure resolution logic behind [`EffectiveRoot`], split out so it is unit-testable
/// without constructing a full `ToolCallContext`. Pops `root` out of `arguments` (so it
/// never reaches the tool's own `Parameters<T>` deserialization) when the server has no
/// configured root, falling back to `configured_root` if absent or invalid.
fn resolve_effective_root(
    root_configured: bool,
    configured_root: &Path,
    arguments: &mut Option<JsonObject>,
) -> PathBuf {
    if root_configured {
        return configured_root.to_path_buf();
    }
    arguments
        .as_mut()
        .and_then(|map| map.remove("root"))
        .and_then(|v| v.as_str().map(PathBuf::from))
        .unwrap_or_else(|| configured_root.to_path_buf())
}

impl FromToolCallContextPart<T0k3nServer> for EffectiveRoot {
    fn from_tool_call_context_part(
        context: &mut ToolCallContext<T0k3nServer>,
    ) -> Result<Self, McpError> {
        let root = resolve_effective_root(
            context.service.config.root_configured,
            &context.service.root,
            &mut context.arguments,
        );
        Ok(EffectiveRoot(root))
    }
}

/// Map a requested zoom + current budget strategy to a concrete detail level.
/// `auto` degrades with budget pressure: critical→skeleton, aggressive→sketch,
/// otherwise body. Explicit levels pass through; anything else is body.
fn zoom_level(requested: Option<&str>, status: Option<&str>) -> &'static str {
    match requested.map(|s| s.to_ascii_lowercase()).as_deref() {
        Some("skeleton") => "skeleton",
        Some("sketch") => "sketch",
        Some("auto") => match status {
            Some("critical") => "skeleton",
            Some("aggressive") => "sketch",
            _ => "body",
        },
        _ => "body",
    }
}

/// File mtime in whole seconds since the epoch, or None if it can't be resolved.
/// Used by the cross-tool content ledger to invalidate references after edits.
fn file_mtime(root: &std::path::Path, rel: &str) -> Option<u64> {
    let abs = crate::security::safe_path(root, rel).ok()?;
    let meta = std::fs::metadata(abs).ok()?;
    let modified = meta.modified().ok()?;
    Some(
        modified
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs(),
    )
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
        && let Some(tc) = v.get("token_count").and_then(|tc| tc.as_u64())
    {
        return Some(tc);
    }
    text.lines().find_map(|l| {
        l.trim()
            .strip_prefix("token_count: ")
            .and_then(|n| n.trim().parse().ok())
    })
}

/// Ledger key for delta reads: tool name + canonical params.
fn delta_key<P: Serialize>(tool: &str, params: &P) -> String {
    format!(
        "{tool}:{}",
        serde_json::to_string(params).unwrap_or_default()
    )
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
    pub fn new(config: ServerConfig, dashboard: Option<Arc<DashboardState>>) -> Self {
        let root_path = PathBuf::from(&config.root);
        let db_path = root_path.join(".t0k3n").join("t0k3n.db");
        let db = Database::new(&db_path).unwrap_or_else(|e| {
            tracing::warn!(
                "Failed to open DB at {:?}: {}. Using in-memory DB.",
                db_path,
                e
            );
            Database::new(std::path::Path::new(":memory:")).unwrap()
        });

        let mut tool_router = Self::tool_router();

        // Optional category profile: every tool schema is carried by the client on
        // every request, so trimming the roster is itself a token optimization.
        if let Some(cats) = &config.tool_categories {
            let keep = tools_in_categories(cats);
            tool_router
                .map
                .retain(|name, _| keep.contains(name.as_ref()));
        }

        // read_type_diagnostics is opt-in: spawning cargo check / tsc / pyright is
        // heavyweight, so it is unregistered (not advertised, not callable) unless
        // explicitly enabled via --enable-diagnostics / T0K3N_ENABLE_DIAGNOSTICS.
        if !config.diagnostics_enabled {
            tool_router.map.remove("read_type_diagnostics");
        }
        // Mutating write tools are opt-in: unregistered unless --enable-writes.
        if !config.writes_enabled {
            for t in WRITE_TOOLS {
                tool_router.map.remove(*t);
            }
        }
        // Shell execution is opt-out: registered unless --disable-commands.
        if !config.commands_enabled {
            for t in COMMAND_TOOLS {
                tool_router.map.remove(*t);
            }
        }
        let tool_count = tool_router.map.len();

        // gen4 warm start: load the cross-session content ledger from disk.
        let content_ledger = ContentLedger::load(&root_path, tools::digest::git_head(&root_path));

        tracing::info!(
            "t0k3n-mcp v{} initialized — {} tools registered \
             (diagnostics: {}, writes: {}, commands: {})",
            env!("CARGO_PKG_VERSION"),
            tool_count,
            if config.diagnostics_enabled {
                "enabled"
            } else {
                "disabled (opt-in)"
            },
            if config.writes_enabled {
                "enabled"
            } else {
                "disabled (opt-in)"
            },
            if config.commands_enabled {
                "enabled"
            } else {
                "disabled"
            },
        );

        Self {
            root: root_path,
            db: Arc::new(Mutex::new(db)),
            web_cache: Arc::new(Mutex::new(HashMap::new())),
            ledger: Arc::new(Mutex::new(ReadLedger::new())),
            cmd_ledger: Arc::new(Mutex::new(CmdLedger::new())),
            content_ledger: Arc::new(Mutex::new(content_ledger)),
            budget_status: Arc::new(Mutex::new(None)),
            tool_router,
            config,
            dashboard,
        }
    }

    /// Render a tool response, consulting the delta-read ledger first.
    /// Repeat reads of unchanged content return a tiny "unchanged" stub;
    /// changed content returns a unified diff when that is cheaper.
    fn ok_delta(&self, key: String, v: serde_json::Value) -> Result<CallToolResult, McpError> {
        let rendered = match output_format() {
            OutputFormat::Json => serde_json::to_string_pretty(&v).map_err(err)?,
            OutputFormat::Compact => tools::render::to_compact_text(&v),
        };
        let delta = self
            .ledger
            .lock()
            .unwrap()
            .check_and_update(&key, &rendered);
        match delta {
            Delta::Full => ok_text(rendered),
            // The stub is only sound while the caller still holds the earlier content.
            // Context compaction can silently break that, and the caller cannot tell
            // from a bare "unchanged" — so publish a digest it can check itself.
            Delta::Unchanged { full_tokens } => ok_json(serde_json::json!({
                "unchanged": true,
                "content_sha256": short_sha256(&rendered),
                "note": "Identical to what you already received earlier this session — content not re-sent. If you no longer have that content in context (e.g. after compaction), do NOT guess: call delta_reset (optionally with a path pattern) and retry.",
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

    /// Cross-tool content dedup: if this exact body (path + line range) was already
    /// sent this session — by any tool — and the file is unchanged, return a tiny
    /// reference stub to put in place of the content. Returns None to send it full
    /// (also recording it so a later re-request can be stubbed).
    fn dedup_body(&self, root: &Path, path: &str, id: &str, content: &str) -> Option<String> {
        let mtime = file_mtime(root, path)?;
        match self
            .content_ledger
            .lock()
            .unwrap()
            .dedup(path, id, content, mtime)
        {
            ContentDedup::Fresh => None,
            ContentDedup::AlreadySent {
                reference,
                full_tokens,
            } => Some(format!(
                "(already sent {reference} earlier this session — identical content not re-sent, ~{full_tokens} tokens saved. Call delta_reset and retry for the full body.)"
            )),
            ContentDedup::UnchangedColdCache {
                reference,
                full_tokens,
            } => Some(format!(
                "(unchanged since a previous session: {reference} — content is NOT in your current context (~{full_tokens} tokens). The file has not changed since you last read it. If you need the body now, call delta_reset and retry.)"
            )),
        }
    }

    // ─────────────────────────────────────────────
    // File reading tools
    // ─────────────────────────────────────────────

    #[tool(
        description = "Get .gitignore-aware directory tree. Use to explore workspace structure before reading files."
    )]
    async fn read_directory_tree(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadDirectoryTreeParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_directory_tree", {
            let key = delta_key("read_directory_tree", &params);
            let result = read_directory_tree(&root, params).map_err(err)?;
            self.ok_delta(
                key,
                serde_json::json!({ "tree": result.tree, "token_count": result.token_count }),
            )
        })
    }

    #[tool(
        description = "Get all headings (TOC) from a Markdown file. Call before read_markdown_section to get anchors."
    )]
    async fn read_markdown_toc(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadMarkdownTocParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_markdown_toc", {
            let key = delta_key("read_markdown_toc", &params);
            let result = read_markdown_toc(&root, params).map_err(err)?;
            self.ok_delta(
                key,
                serde_json::json!({ "toc": result.toc, "token_count": result.token_count }),
            )
        })
    }

    #[tool(
        description = "Get specific sections from a Markdown file by anchor. Call read_markdown_toc first to get anchors."
    )]
    async fn read_markdown_section(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadMarkdownSectionParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_markdown_section", {
            let key = delta_key("read_markdown_section", &params);
            let result = read_markdown_section(&root, params).map_err(err)?;
            self.ok_delta(key, serde_json::json!({ "sections": result.sections, "token_count": result.token_count }))
        })
    }

    #[tool(
        description = "Write/edit a Markdown section by heading anchor (opt-in write tool; requires --enable-writes) — write counterpart of read_markdown_toc / read_markdown_section. mode: 'replace' (swap an existing section's full text, heading included), 'insert_before'/'insert_after' (add a new block relative to anchor's section), 'append' (add at end of file, anchor not required), or 'delete' (remove the section). Pass expected_title to guard against a stale TOC. dry_run previews the diff."
    )]
    async fn write_markdown_section(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<WriteMarkdownSectionParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "write_markdown_section", {
            let result = write_markdown_section(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "diff": result.diff,
                "written": result.written,
                "token_count": tools::fs::estimate_tokens(&result.diff),
            }))
        })
    }

    #[tool(
        description = "Search a file for a keyword or regex pattern with surrounding context lines."
    )]
    async fn search_file(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<SearchFileParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "search_file", {
            let result = search_file(&root, params).map_err(err)?;
            ok_json(
                serde_json::json!({ "matches": result.matches, "token_count": result.token_count }),
            )
        })
    }

    #[tool(
        description = "Get key structure of a JSON or YAML file. Call before read_json_yaml_value to identify key paths."
    )]
    async fn read_json_yaml_keys(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadJsonYamlKeysParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_json_yaml_keys", {
            let key = delta_key("read_json_yaml_keys", &params);
            let result = read_json_yaml_keys(&root, params).map_err(err)?;
            self.ok_delta(
                key,
                serde_json::json!({ "keys": result.keys, "token_count": result.token_count }),
            )
        })
    }

    #[tool(
        description = "Get a specific value from a JSON or YAML file by dot-notation key path (e.g. 'dependencies.tokio')."
    )]
    async fn read_json_yaml_value(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadJsonYamlValueParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_json_yaml_value", {
            let key = delta_key("read_json_yaml_value", &params);
            let result = read_json_yaml_value(&root, params).map_err(err)?;
            self.ok_delta(
                key,
                serde_json::json!({ "value": result.value, "token_count": result.token_count }),
            )
        })
    }

    #[tool(
        description = "Get code skeleton (functions, structs, classes) with signatures only. Call before read_code_body."
    )]
    async fn read_code_skeleton(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadCodeSkeletonParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_code_skeleton", {
            let key = delta_key("read_code_skeleton", &params);
            let result = read_code_skeleton(&root, params).map_err(err)?;
            self.ok_delta(key, serde_json::json!({ "language": result.language, "skeleton": result.skeleton, "token_count": result.token_count }))
        })
    }

    /// Resolve the requested zoom into a concrete detail level. `auto` consults
    /// the latest check_budget strategy; anything unrecognized falls back to body.
    fn resolve_zoom(&self, requested: Option<&str>) -> &'static str {
        let status = self.budget_status.lock().ok().and_then(|s| s.clone());
        zoom_level(requested, status.as_deref())
    }

    #[tool(
        description = "Get full body of specific code items by ID from read_code_skeleton. Optional zoom controls detail: 'body' (default), 'sketch' (control-flow only), 'skeleton' (signatures only), or 'auto' (pick by the latest check_budget strategy). The chosen level is echoed back as zoom_applied."
    )]
    async fn read_code_body(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadCodeBodyParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_code_body", {
            let level = self.resolve_zoom(params.zoom.as_deref());
            let path = params.path.clone();
            let ids = params.ids.clone();

            match level {
                "skeleton" => {
                    let key = delta_key("read_code_body:skeleton", &path);
                    let result = read_code_skeleton(
                        &root,
                        ReadCodeSkeletonParams {
                            path: path.clone(),
                            include_blocks: None,
                        },
                    )
                    .map_err(err)?;
                    self.ok_delta(
                        key,
                        serde_json::json!({
                            "zoom_applied": "skeleton",
                            "language": result.language,
                            "skeleton": result.skeleton,
                            "token_count": result.token_count,
                        }),
                    )
                }
                "sketch" => {
                    let sk_params = ReadCodeSketchParams {
                        path: path.clone(),
                        ids,
                    };
                    let key = delta_key("read_code_body:sketch", &sk_params);
                    let result = read_code_sketch(&root, sk_params).map_err(err)?;
                    self.ok_delta(
                        key,
                        serde_json::json!({
                            "zoom_applied": "sketch",
                            "items": result.items,
                            "token_count": result.token_count,
                        }),
                    )
                }
                _ => {
                    let key = delta_key("read_code_body", &params);
                    let mut result = read_code_body(&root, params).map_err(err)?;
                    // Cross-tool dedup: stub bodies already sent this session (e.g. by read_context_pack).
                    for item in &mut result.items {
                        if item.content.starts_with("Error:") {
                            continue;
                        }
                        if let Some(stub) = self.dedup_body(&root, &path, &item.id, &item.content) {
                            item.content = stub;
                        }
                    }
                    let token_count = tools::fs::estimate_tokens(
                        &serde_json::to_string(&result.items).unwrap_or_default(),
                    );
                    self.ok_delta(
                        key,
                        serde_json::json!({
                            "zoom_applied": "body",
                            "items": result.items,
                            "token_count": token_count,
                        }),
                    )
                }
            }
        })
    }

    #[tool(
        description = "Zoom level between read_code_skeleton (signatures) and read_code_body (full source). Given skeleton IDs, returns each symbol's control-flow sketch: signature + branches/loops + block delimiters + call lines kept verbatim, runs of pure-data lines (assignments, literals) collapsed into '… N lines …'. Typically 60-70% smaller than the body — use it to understand what a function does before deciding whether you need the full body."
    )]
    async fn read_code_sketch(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadCodeSketchParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_code_sketch", {
            let key = delta_key("read_code_sketch", &params);
            let result = read_code_sketch(&root, params).map_err(err)?;
            self.ok_delta(
                key,
                serde_json::json!({ "items": result.items, "token_count": result.token_count }),
            )
        })
    }

    #[tool(
        description = "Replace one symbol's source by skeleton ID — write counterpart of read_code_body. Flow: read_code_skeleton → read_code_body(id) → patch_symbol(id, new_body|edits). For small changes pass edits=[{find,replace}] instead of new_body — find only needs to be unique within the symbol, so unchanged lines are never resent. Pass expected_name to guard against stale line numbers; re-run read_code_skeleton after each successful patch before patching the same file again. dry_run previews the diff."
    )]
    async fn patch_symbol(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<PatchSymbolParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "patch_symbol", {
            let result = patch_symbol(&root, params).map_err(err)?;
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

    #[tool(
        description = "Rename a symbol across the whole workspace in one call — write counterpart of read_symbol_usages. Whole-identifier match only (substrings like old_name_extended are left untouched). Returns affected file count + per-line before/after edits, never full file bodies. Always run once with dry_run:true to preview scope before applying. Scope to a file/dir with path. Note: textual whole-word match (same basis as read_symbol_usages) — it does not skip identical names in comments or strings, so review the dry_run output."
    )]
    async fn rename_symbol(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<RenameSymbolParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "rename_symbol", {
            let result = rename_symbol(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "applied": result.applied,
                "files_changed": result.files_changed,
                "occurrences": result.occurrences,
                "changes": result.changes,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Create a new file (opt-in write tool; requires --enable-writes). Refuses to overwrite an existing file unless overwrite:true. Creates parent directories. dry_run reports what would happen without writing. Fills the gap where the only way to create a file was run_command."
    )]
    async fn create_file(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<CreateFileParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "create_file", {
            let result = create_file(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path,
                "bytes": result.bytes,
                "created": result.created,
                "overwritten": result.overwritten,
                "written": result.written,
                "token_count": 20,
            }))
        })
    }

    #[tool(
        description = "Delete a symbol by skeleton ID (opt-in write tool; requires --enable-writes) — write counterpart of read_dead_code. Removes the symbol's line range plus one trailing blank line. Pass expected_name to guard against stale line numbers; dry_run previews the diff."
    )]
    async fn delete_symbol(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<DeleteSymbolParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "delete_symbol", {
            let result = delete_symbol(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "removed_lines": result.removed_lines,
                "diff": result.diff,
                "written": result.written,
                "token_count": tools::fs::estimate_tokens(&result.diff),
            }))
        })
    }

    #[tool(
        description = "Insert code at a structurally correct location (opt-in write tool; requires --enable-writes). mode: 'after_symbol'/'before_symbol' (need anchor_id from read_code_skeleton), 'after_imports' (after the import block), or 'end_of_file'. Adds blank-line separation automatically. dry_run previews the diff. Completes symbol CRUD with patch_symbol (update) and delete_symbol (delete)."
    )]
    async fn insert_symbol(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<InsertSymbolParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "insert_symbol", {
            let result = insert_symbol(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "inserted_at_line": result.inserted_at_line,
                "diff": result.diff,
                "written": result.written,
                "token_count": tools::fs::estimate_tokens(&result.diff),
            }))
        })
    }

    #[tool(
        description = "Apply find/replace edits across one or more files atomically (opt-in write tool; requires --enable-writes) — write counterpart of batch_read. Each find must match exactly once per file (ambiguous matches report candidate line numbers). If any edit fails, nothing is written. Returns per-edit line + before/after summaries only. dry_run validates and previews without writing."
    )]
    async fn apply_edits(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ApplyEditsParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "apply_edits", {
            let result = apply_edits(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "files_changed": result.files_changed,
                "edits_applied": result.edits_applied,
                "changes": result.changes,
                "written": result.written,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Set a value at a dot-notation key path in a JSON/YAML/TOML file (opt-in write tool; requires --enable-writes) — write counterpart of read_json_yaml_value. Creates intermediate objects as needed; value may be any JSON type. JSON key order is preserved; YAML/TOML comments are not. dry_run previews the diff. Returns old/new value + diff only."
    )]
    async fn set_config_value(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<SetConfigValueParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "set_config_value", {
            let result = set_config_value(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "old_value": result.old_value,
                "new_value": result.new_value,
                "created": result.created,
                "diff": result.diff,
                "written": result.written,
                "token_count": tools::fs::estimate_tokens(&result.diff),
            }))
        })
    }

    #[tool(
        description = "Add or remove import statements (opt-in write tool; requires --enable-writes). Operates on whole import lines (language-agnostic): adds at the import block, removes by trimmed equality, and de-duplicates against existing imports. dry_run previews the diff."
    )]
    async fn manage_imports(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ManageImportsParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "manage_imports", {
            let result = manage_imports(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "added": result.added,
                "removed": result.removed,
                "skipped": result.skipped,
                "diff": result.diff,
                "written": result.written,
                "token_count": tools::fs::estimate_tokens(&result.diff),
            }))
        })
    }

    #[tool(
        description = "Run the language's formatter on a file (opt-in write tool; requires --enable-writes): rustfmt / prettier / black / gofmt by extension. Returns the diff and whether anything changed. dry_run formats a copy and previews without writing. If the formatter is not installed, returns formatter_available:false + an install hint (no error)."
    )]
    async fn format_code(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<FormatCodeParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "format_code", {
            let result = format_code(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "formatter": result.formatter,
                "formatter_available": result.formatter_available,
                "changed": result.changed,
                "diff": result.diff,
                "written": result.written,
                "note": result.note,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Move a symbol from one file to another by skeleton ID (opt-in write tool; requires --enable-writes). Extracts it from src_path and appends to dest_path (created if missing). Import fixups are best-effort: imports are NOT rewritten, but referencing files are reported in warnings. Pass symbol_name for a stale-line guard + the reference-impact warning. dry_run previews both diffs."
    )]
    async fn move_symbol(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<MoveSymbolParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "move_symbol", {
            let result = move_symbol(&root, params).map_err(err)?;
            let tok = tools::fs::estimate_tokens(&result.src_diff)
                + tools::fs::estimate_tokens(&result.dest_diff);
            ok_json(serde_json::json!({
                "moved_lines": result.moved_lines,
                "dest_created": result.dest_created,
                "src_diff": result.src_diff,
                "dest_diff": result.dest_diff,
                "warnings": result.warnings,
                "written": result.written,
                "token_count": tok,
            }))
        })
    }

    #[tool(
        description = "Snapshot the working tree before a batch of edits (opt-in write tool; requires --enable-writes) — safety net for autonomous write loops. In a git repo uses `git stash create` (does not touch the tree); otherwise copies gitignore-aware files into .t0k3n/checkpoints/. Returns a checkpoint_id to pass to rollback. Distinct from session_snapshot (which saves tool state, not files)."
    )]
    async fn edit_checkpoint(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<EditCheckpointParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "edit_checkpoint", {
            let result = edit_checkpoint(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "checkpoint_id": result.checkpoint_id,
                "strategy": result.strategy,
                "files": result.files,
                "note": result.note,
                "token_count": 20,
            }))
        })
    }

    #[tool(
        description = "Restore the working tree to a prior edit_checkpoint (opt-in write tool; requires --enable-writes). Pass the checkpoint_id from edit_checkpoint. git checkpoints restore tracked files via `git checkout`; copy checkpoints copy files back. Note: files created after the checkpoint are not removed."
    )]
    async fn rollback(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<RollbackParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "rollback", {
            let result = rollback(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "strategy": result.strategy,
                "restored": result.restored,
                "note": result.note,
                "token_count": 20,
            }))
        })
    }

    #[tool(
        description = "Get import/dependency graph for a code file. Returns what it imports and what files import it (imported_by). direction: \"imports\" | \"imported_by\" | \"both\"."
    )]
    async fn read_code_deps(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadCodeDepsParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_code_deps", {
            let result = read_code_deps(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path, "language": result.language,
                "imports": result.imports, "imported_by": result.imported_by,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Get a unified outline of any file. Auto-detects type: code → skeleton, markdown → TOC, json/yaml → keys. Single entry point — no need to know the file type first."
    )]
    async fn read_file_outline(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadFileOutlineParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_file_outline", {
            let key = delta_key("read_file_outline", &params);
            let result = read_file_outline(&root, params).map_err(err)?;
            self.ok_delta(
                key,
                serde_json::json!({
                    "path": result.path, "kind": result.kind, "language": result.language,
                    "outline": result.outline, "token_count": result.token_count,
                }),
            )
        })
    }

    #[tool(
        description = "One-call task context collection: ranks workspace files and symbols by relevance to a task description, returns ranked files + relevant signatures + top symbol bodies, greedily filled up to a token budget. Replaces the tree→search→skeleton→body round-trip sequence when starting a task. No subprocess needed (lexical ranking)."
    )]
    async fn read_context_pack(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadContextPackParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_context_pack", {
            let mut result = read_context_pack(&root, params).map_err(err)?;
            // Record each body in the cross-tool ledger so a later read_code_body for
            // the same symbol is stubbed; stub here too if it was already sent.
            for body in &mut result.bodies {
                if let Some(stub) = self.dedup_body(&root, &body.path, &body.id, &body.content) {
                    body.content = stub;
                }
            }
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

    #[tool(
        description = "Search code semantically using a natural language query. EXPENSIVE AND NOT A GREP SUBSTITUTE: this spawns a separate `claude -p` CLI process, which is a billed model call of its own, adds seconds of latency, and gives non-deterministic results. Requires the `claude` CLI installed and authenticated. Prefer search_file (regex) or read_code_skeleton + read_code_body when you can name what you are looking for; reach for this only when the query is genuinely conceptual."
    )]
    async fn semantic_search(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<SemanticSearchParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "semantic_search", {
            let result = semantic_search(&root, params).map_err(err)?;
            ok_json(serde_json::json!({ "items": result.items, "token_count": result.token_count }))
        })
    }

    #[tool(
        description = "Get compressed git diff. Defaults to all uncommitted changes vs HEAD. Use stat_only for a quick file-level summary. zoom mirrors read_code_body: 'body' (full diff), 'sketch' (file + hunk headers only), 'skeleton' (per-file × enclosing-symbol +/- line counts, no diff text), or 'auto' (follows the latest check_budget strategy). Apply the structure-first read to change itself: skeleton to map a large diff, then body on the suspicious files."
    )]
    async fn read_git_diff(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(mut params): Parameters<ReadGitDiffParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_git_diff", {
            // Resolve `auto` (and any synonym) against the latest budget strategy
            // before handing a concrete level to the stateless tool fn.
            params.zoom = Some(self.resolve_zoom(params.zoom.as_deref()).to_string());
            let result = read_git_diff(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "diff": result.diff,
                "files": result.files,
                "zoom_applied": result.zoom_applied,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Get structured git commit log with sha, author, date, message, and changed files. Filter by path, author, date range, or limit."
    )]
    async fn read_git_log(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadGitLogParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_git_log", {
            let result = read_git_log(&root, params).map_err(err)?;
            ok_json(
                serde_json::json!({ "entries": result.entries, "token_count": result.token_count }),
            )
        })
    }

    #[tool(
        description = "Get per-line blame (author + date) for a specific line range in a file. Use start_line/end_line from read_code_skeleton to target a function body."
    )]
    async fn read_git_blame_body(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadGitBlameBodyParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_git_blame_body", {
            let result = read_git_blame_body(&root, params).map_err(err)?;
            ok_json(
                serde_json::json!({ "path": result.path, "lines": result.lines, "token_count": result.token_count }),
            )
        })
    }

    #[tool(
        description = "Find all usages of a symbol name (function, struct, class, variable) across the workspace. Returns file path, line number, and context for each match. Max 100 results."
    )]
    async fn read_symbol_usages(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadSymbolUsagesParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_symbol_usages", {
            let result = read_symbol_usages(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "symbol": result.symbol, "usages": result.usages,
                "total": result.total, "truncated": result.truncated,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Parse an OpenAPI / Swagger spec (JSON or YAML) and return a compact endpoint summary: method, path, operation_id, summary, parameters, request body, and responses."
    )]
    async fn read_openapi(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadOpenApiParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_openapi", {
            let result = read_openapi(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "title": result.title, "version": result.version,
                "base_url": result.base_url, "spec_version": result.spec_version,
                "endpoints": result.endpoints, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Extract environment variable definitions from .env.example / .env.sample / .env.template / docker-compose.yml. Returns key, description (from comments), default value, and required flag. Omit path to auto-scan workspace root."
    )]
    async fn read_env_schema(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadEnvSchemaParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_env_schema", {
            let result = read_env_schema(&root, params).map_err(err)?;
            ok_json(
                serde_json::json!({ "vars": result.vars, "sources": result.sources, "token_count": result.token_count }),
            )
        })
    }

    // ─────────────────────────────────────────────
    // Web tools
    // ─────────────────────────────────────────────

    #[tool(
        description = "Fetch a webpage, convert HTML to Markdown, return TOC only. Call read_webpage_section to read specific sections."
    )]
    async fn fetch_webpage(
        &self,
        Parameters(params): Parameters<FetchWebpageParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "fetch_webpage", {
            let cache = self.web_cache.clone();
            let result = fetch_webpage(params, cache).await.map_err(err)?;
            ok_json(
                serde_json::json!({ "toc": result.toc, "token_count": result.token_count, "cached": result.cached }),
            )
        })
    }

    #[tool(
        description = "Get specific sections from a cached webpage by anchor. Call fetch_webpage first."
    )]
    async fn read_webpage_section(
        &self,
        Parameters(params): Parameters<ReadWebpageSectionParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_webpage_section", {
            let cache = self.web_cache.clone();
            let result = read_webpage_section(params, cache).map_err(err)?;
            ok_json(
                serde_json::json!({ "sections": result.sections, "token_count": result.token_count }),
            )
        })
    }

    // ─────────────────────────────────────────────
    // Document conversion
    // ─────────────────────────────────────────────

    #[tool(
        description = "Convert a PDF or DOCX to Markdown, return TOC and tmp_path. Use read_markdown_section(tmp_path) to read sections."
    )]
    async fn convert_document(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ConvertDocumentParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "convert_document", {
            let result = convert_document(&root, params).map_err(err)?;
            ok_json(
                serde_json::json!({ "toc": result.toc, "tmp_path": result.tmp_path, "token_count": result.token_count }),
            )
        })
    }

    // ─────────────────────────────────────────────
    // Text tools
    // ─────────────────────────────────────────────

    #[tool(
        description = "Compress text by removing excessive whitespace and noise. Returns compressed text with token stats."
    )]
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

    #[tool(
        description = "Check token budget and get reading strategy (normal/conservative/aggressive/critical)."
    )]
    async fn check_budget(
        &self,
        Parameters(params): Parameters<CheckBudgetParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "check_budget", {
            let result = check_budget(params);
            // Remember the strategy so read_code_body's zoom:auto can use it.
            if let Ok(mut s) = self.budget_status.lock() {
                *s = Some(result.strategy.clone());
            }
            ok_json(result)
        })
    }

    #[tool(description = "Summarize conversation text to fit within a token budget.")]
    async fn summarize_conversation(
        &self,
        Parameters(params): Parameters<SummarizeConversationParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "summarize_conversation", {
            ok_json(summarize_conversation(params))
        })
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

    #[tool(
        description = "Create a task with title, description, status (pending/in_progress/done/cancelled), priority, tags."
    )]
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

    #[tool(
        description = "Save a snapshot of work state (arbitrary JSON) for restoration in future sessions."
    )]
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

    #[tool(
        description = "Get table/model list from a Prisma or SQL schema file. Returns name, kind, and field count. Call read_db_table for field details of a specific table."
    )]
    async fn read_db_schema(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadDbSchemaParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_db_schema", {
            let result = read_db_schema(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path, "format": result.format,
                "tables": result.tables, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Get full field definitions for a specific table or model from a Prisma or SQL schema. Call read_db_schema first to get the table list."
    )]
    async fn read_db_table(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadDbTableParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_db_table", {
            let result = read_db_table(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "name": result.name, "kind": result.kind,
                "fields": result.fields, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Get CSS/SCSS/Less selector list with property counts. Returns IDs for use with read_css_body."
    )]
    async fn read_css_skeleton(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadCssSkeletonParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_css_skeleton", {
            let result = read_css_skeleton(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path, "selectors": result.selectors, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Get full CSS rule content for specific selectors by ID. Call read_css_skeleton first to get selector IDs."
    )]
    async fn read_css_body(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadCssBodyParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_css_body", {
            let result = read_css_body(&root, params).map_err(err)?;
            ok_json(serde_json::json!({ "items": result.items, "token_count": result.token_count }))
        })
    }

    #[tool(
        description = "Get type/input/enum/interface list from a GraphQL schema file. Returns IDs for use with read_graphql_type."
    )]
    async fn read_graphql_schema(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadGraphqlSchemaParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_graphql_schema", {
            let result = read_graphql_schema(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path, "types": result.types, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Get full field definitions for a specific GraphQL type. Call read_graphql_schema first to get the type list."
    )]
    async fn read_graphql_type(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadGraphqlTypeParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_graphql_type", {
            let result = read_graphql_type(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "name": result.name, "kind": result.kind,
                "fields": result.fields, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Get message/service/enum list from a .proto (Protocol Buffers) file. Returns IDs for use with read_proto_type."
    )]
    async fn read_proto_schema(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadProtoSchemaParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_proto_schema", {
            let result = read_proto_schema(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path, "syntax": result.syntax, "package": result.package,
                "types": result.types, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Get full field definitions for a specific message, service, or enum in a .proto file. Call read_proto_schema first to get the type list."
    )]
    async fn read_proto_type(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadProtoTypeParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_proto_type", {
            let result = read_proto_type(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "name": result.name, "kind": result.kind,
                "fields": result.fields, "token_count": result.token_count,
            }))
        })
    }

    // ─────────────────────────────────────────────
    // Notebook tools
    // ─────────────────────────────────────────────

    #[tool(
        description = "Get cell list from a Jupyter notebook (.ipynb) with type, preview, and output count. Call before read_notebook_cell to choose which cells to read."
    )]
    async fn read_notebook_cells(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadNotebookCellsParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_notebook_cells", {
            let result = read_notebook_cells(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path, "nbformat": result.nbformat,
                "cells": result.cells, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Get full source of a specific cell from a Jupyter notebook (.ipynb). Use the index from read_notebook_cells. Set include_outputs=true to also fetch cell outputs."
    )]
    async fn read_notebook_cell(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadNotebookCellParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_notebook_cell", {
            let result = read_notebook_cell(&root, params).map_err(err)?;
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

    #[tool(
        description = "Read the tail of a log file with optional level (ERROR/WARN/INFO/DEBUG) and regex pattern filters. Returns last N lines and level counts across the whole file."
    )]
    async fn read_log_tail(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadLogTailParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_log_tail", {
            let result = read_log_tail(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path, "total_lines": result.total_lines,
                "returned_lines": result.returned_lines, "level_counts": result.level_counts,
                "lines": result.lines, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Parse a stack trace and fetch source context around each referenced file:line. Supports Python, Rust, JavaScript/TypeScript, Java, Go, and C#. Returns resolved code snippets from workspace files."
    )]
    async fn read_stack_trace(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadStackTraceParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_stack_trace", {
            let result = read_stack_trace(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "total_frames": result.total_frames, "resolved_frames": result.resolved_frames,
                "frames": result.frames, "token_count": result.token_count,
            }))
        })
    }

    // ─────────────────────────────────────────────
    // Test tools
    // ─────────────────────────────────────────────

    #[tool(
        description = "Get test case list from a test file (Jest/pytest/Rust/#[test]/Go/JUnit/RSpec). Returns IDs usable with read_code_body to get test implementations."
    )]
    async fn read_test_skeleton(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadTestSkeletonParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_test_skeleton", {
            let result = read_test_skeleton(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path, "framework": result.framework,
                "tests": result.tests, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Parse test runner output (Jest/Vitest/pytest/cargo test/go test) into a structured summary: pass/fail counts per suite and failure details. Accepts raw text or a file path."
    )]
    async fn read_test_results(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadTestResultsParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_test_results", {
            let result = read_test_results(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "framework": result.framework, "summary": result.summary,
                "suites": result.suites, "failures": result.failures,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Map a coverage report onto code symbols to see which functions are untested (risky to change). Auto-detects lcov (lcov.info / cargo llvm-cov), coverage.py JSON, or cobertura XML. Per-symbol covered/total/pct plus overall_pct. Filter with uncovered_only (pct<100) or threshold. If no report exists, returns report_available:false + a generation hint (safe to call speculatively). Pairs with read_test_results / read_test_skeleton."
    )]
    async fn read_test_coverage(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadTestCoverageParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_test_coverage", {
            let result = read_test_coverage(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "report_available": result.report_available,
                "format": result.format,
                "overall_pct": result.overall_pct,
                "files": result.files,
                "hint": result.hint,
                "token_count": result.token_count,
            }))
        })
    }

    // ─────────────────────────────────────────────
    // Extended code analysis tools
    // ─────────────────────────────────────────────

    #[tool(
        description = "Get type definitions (interface/type/enum/struct) with field names from TypeScript, Go, or Rust files. More detailed than read_code_skeleton for type-heavy files."
    )]
    async fn read_type_skeleton(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadTypeSkeletonParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_type_skeleton", {
            let key = delta_key("read_type_skeleton", &params);
            let result = read_type_skeleton(&root, params).map_err(err)?;
            self.ok_delta(
                key,
                serde_json::json!({
                    "path": result.path, "language": result.language,
                    "types": result.types, "token_count": result.token_count,
                }),
            )
        })
    }

    #[tool(
        description = "Get the call graph for a function: what functions it calls, and which functions in the same file call it. Uses function_id from read_code_skeleton."
    )]
    async fn read_call_graph(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadCallGraphParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_call_graph", {
            let result = read_call_graph(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "function": result.function, "file": result.file,
                "calls": result.calls, "called_by_in_file": result.called_by_in_file,
                "cross_file_callees": result.cross_file_callees,
                "cross_file_callers": result.cross_file_callers,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "List all workspace files sorted by estimated token count (largest first). Use to identify token-heavy files before reading. Supports glob filtering."
    )]
    async fn read_token_map(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadTokenMapParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_token_map", {
            let result = read_token_map(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "files": result.files, "total_tokens": result.total_tokens,
                "file_count": result.file_count, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Get per-file change summary (added/deleted lines, status) for the current diff vs a base ref. Step 1 before read_git_diff — get the file list first, then read specific files' diffs."
    )]
    async fn read_changed_files(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadChangedFilesParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_changed_files", {
            let result = read_changed_files(&root, params).map_err(err)?;
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

    #[tool(
        description = "List stashes and optionally get diff for a specific stash entry. Omit index to list only."
    )]
    async fn read_git_stash(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadGitStashParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_git_stash", {
            let result = read_git_stash(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "stashes": result.stashes, "diff": result.diff, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Fuse git log + blame into code ownership: per file, churn (commit count), the date it was last touched, and top authors by lines contributed (ownership share). Sorted by churn to surface hotspots. Use to learn who to ask about a file and where the volatile code is. Scope with path, limit with top_n, window with since (e.g. \"3 months ago\")."
    )]
    async fn read_code_ownership(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadCodeOwnershipParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_code_ownership", {
            let result = read_code_ownership(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "hotspots": result.hotspots, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Parse package.json / Cargo.toml / go.mod / pyproject.toml / pom.xml / build.gradle into a unified dependency list. Faster than read_json_yaml_value for multi-ecosystem projects."
    )]
    async fn read_package_manifest(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadPackageManifestParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_package_manifest", {
            let result = read_package_manifest(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "manifests": result.manifests, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Parse CI pipeline configs (GitHub Actions / GitLab CI / CircleCI) into structured workflow/job/step summary. Omit path to auto-scan workspace."
    )]
    async fn read_ci_pipeline(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadCiPipelineParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_ci_pipeline", {
            let result = read_ci_pipeline(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "pipelines": result.pipelines, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Get codebase-wide statistics: total files/lines/tokens, per-language breakdown with %, and top-10 largest files. Much faster overview than read_token_map."
    )]
    async fn read_workspace_stats(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadWorkspaceStatsParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_workspace_stats", {
            let result = read_workspace_stats(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "total_files": result.total_files, "total_lines": result.total_lines,
                "total_tokens": result.total_tokens, "by_language": result.by_language,
                "largest_files": result.largest_files, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Find all types that implement a given interface/trait/abstract class across the workspace. Supports TypeScript, Rust, Java, Kotlin, Go, PHP, C#."
    )]
    async fn read_interface_conformance(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadInterfaceConformanceParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_interface_conformance", {
            let result = read_interface_conformance(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "name": result.name, "kind": result.kind,
                "implementations": result.implementations,
                "total": result.total, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Execute multiple read operations in one call (code_skeleton | code_body | markdown_section | json_value | file_outline). Reduces round-trips when you need several files at once. Pass factor:true to collapse near-identical results (migrations, fixtures) into one template + per-file unified diffs."
    )]
    async fn batch_read(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<BatchReadParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "batch_read", {
            let result = batch_read(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "results": result.results,
                "factored": result.factored,
                "total_token_count": result.total_token_count,
            }))
        })
    }

    // ─────────────────────────────────────────────
    // Phase 5 — Differentiating analysis tools
    // ─────────────────────────────────────────────

    #[tool(
        description = "Compute cyclomatic complexity for every function in a file or directory. Returns functions sorted by complexity with risk level (low/medium/high/critical). Use to identify refactoring targets without running a linter."
    )]
    async fn read_complexity_map(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadComplexityMapParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_complexity_map", {
            let result = read_complexity_map(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "entries": result.entries,
                "total_analyzed": result.total_analyzed,
                "high_risk_count": result.high_risk_count,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Find unused symbols (functions, classes, structs) that are defined but never called across the workspace. Works across all tree-sitter supported languages without a compiler or LSP. HEURISTIC name-based matching: trait/interface impls, dynamic dispatch and reflection targets can look unused, so each entry carries a `confidence` — confirm before deleting."
    )]
    async fn read_dead_code(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadDeadCodeParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_dead_code", {
            let result = read_dead_code(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "entries": result.entries,
                "total_symbols_checked": result.total_symbols_checked,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Blast-radius analysis for a refactor: given a symbol name, returns all callers, all files that reference it, and all test files that cover it — in one call. Combines call_graph + symbol_usages + test discovery."
    )]
    async fn read_refactor_impact(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadRefactorImpactParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_refactor_impact", {
            let result = read_refactor_impact(&root, params).map_err(err)?;
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

    #[tool(
        description = "Static security surface scan: flags potential injection vectors, XSS sinks, hardcoded secrets, unsafe code, and path traversal patterns. HEURISTIC line-pattern matcher, not taint analysis — every finding carries `severity` (impact if real) AND `confidence` (how likely it is real); verify anything below high confidence by reading the code. Test code is skipped unless include_tests:true; pass min_confidence to cut noise. Categories: injection, xss, secrets, unsafe, path_traversal, all."
    )]
    async fn read_security_surface(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadSecuritySurfaceParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_security_surface", {
            let result = read_security_surface(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "findings": result.findings,
                "total": result.total,
                "by_category": result.by_category,
                "by_severity": result.by_severity,
                "by_confidence": result.by_confidence,
                "note": result.note,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Scan dependencies for known vulnerabilities — the dependency-side counterpart to read_security_surface. Auto-detects the ecosystem (Cargo.toml→cargo audit, package.json→npm audit, pyproject/requirements→pip-audit, go.mod→osv-scanner) and normalizes results to {package, severity, id, affected, patched, title}, sorted by severity. Filter with severity (minimum level) / max_items. If the scanner is not installed, returns scanner_available:false + an install hint (safe to call speculatively)."
    )]
    async fn read_dependency_audit(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadDependencyAuditParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_dependency_audit", {
            let result = read_dependency_audit(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "scanner_available": result.scanner_available,
                "ecosystem": result.ecosystem,
                "vulnerabilities": result.vulnerabilities,
                "hint": result.hint,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Extract only a codebase's public API surface: Rust pub items, TS/JS exports, Python __all__ / non-underscore top-level defs, Go capitalized identifiers. Signatures only (no bodies). Use to understand a library's external boundary or to detect breaking changes (pair with diff_schemas). Scope with path; include_crate_visible:true also lists Rust pub(crate)/pub(super)."
    )]
    async fn read_api_surface(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadApiSurfaceParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_api_surface", {
            let result = read_api_surface(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "api": result.api,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Diff a schema file (OpenAPI, Prisma/SQL, TypeScript types) between two git refs. Returns added/removed/modified endpoints, tables, or types. Auto-detects schema type from file extension."
    )]
    async fn diff_schemas(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<DiffSchemasParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "diff_schemas", {
            let result = diff_schemas(&root, params).map_err(err)?;
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

    #[tool(
        description = "Load full PR context in one call: changed files with skeletons, diff stats, related test files, and commit list. Pass branch + base to get everything needed for a code review without multiple round-trips."
    )]
    async fn read_pr_context(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadPrContextParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_pr_context", {
            let result = read_pr_context(&root, params).map_err(err)?;
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

    #[tool(
        description = "Static type diagnostics (LSP-equivalent) without running a language server. OPT-IN: this tool is only registered when the server is started with --enable-diagnostics (or T0K3N_ENABLE_DIAGNOSTICS=1), because it spawns the language toolchain. Drives the language's own check-only engine — cargo check (Rust), tsc --noEmit (TypeScript), pyright/mypy (Python), go vet (Go) — and returns a compact, deduplicated list of {file, line, col, severity, code, message}. Auto-detects the language from the manifest/extension; pass `language` to force it, `path` to scope to a file/dir, `severity` (error|warning|hint) as a floor, and `max_items` to cap. If the checker is not installed it returns checker_available:false with an install hint instead of erroring."
    )]
    async fn read_type_diagnostics(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadTypeDiagnosticsParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_type_diagnostics", {
            if !self.config.diagnostics_enabled {
                return ok_json(serde_json::json!({
                    "error": "read_type_diagnostics is disabled. Restart the server with --enable-diagnostics (or set T0K3N_ENABLE_DIAGNOSTICS=1) to use it.",
                    "token_count": 30,
                }));
            }
            let result = read_type_diagnostics(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "language": result.language,
                "checker": result.checker,
                "checker_available": result.checker_available,
                "note": result.note,
                "diagnostics": result.diagnostics,
                "summary": result.summary,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Warm-start project digest: a cached ~2k-token architecture summary (git HEAD, language stats, entry-point files with their top symbols, shallow directory tree) returned in one call. Replaces the repeated tree → stats → skeleton exploration at session start. The cache (.t0k3n/digest.json) auto-invalidates when git HEAD changes; pass refresh:true to rebuild. `dirty` flags an uncommitted working tree (digest may be stale)."
    )]
    async fn project_digest(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ProjectDigestParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "project_digest", {
            let result = project_digest(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "cached": result.cached,
                "dirty": result.dirty,
                "digest": result.digest,
                "token_count": result.token_count,
            }))
        })
    }

    // ─────────────────────────────────────────────
    // Debug tool
    // ─────────────────────────────────────────────

    #[tool(
        description = "Execute a shell command and return token-efficient output. On success: last ~30 lines (final summary). On failure: extracted error lines + warning lines + last ~20 lines for context. Use for build tools (cargo, npm, go, make, mvn), test runners (cargo test, pytest, jest), linters (clippy, eslint, flake8), and type checkers (tsc, mypy). Repeat runs of the same command return only the delta: new/resolved/unchanged error and warning counts plus the new lines — unchanged lines equal what you already received. Call delta_reset and rerun for full output."
    )]
    async fn run_command(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<RunCommandParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "run_command", {
            let key = CmdLedger::key(&params.command, params.cwd.as_deref());
            let result = run_command(&root, params).map_err(err)?;
            let delta = self
                .cmd_ledger
                .lock()
                .unwrap()
                .check_and_update(&key, &result);
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

    #[tool(
        description = "Reset the delta ledgers (delta reads, run_command deltas, AND the cross-tool content ledger). After this, read tools return full content and run_command returns full output again instead of 'unchanged'/diff/delta/'already sent' stubs. Call when you no longer have earlier tool output in context (e.g. after conversation compaction). Optional pattern narrows the reset to matching keys (e.g. a file path or command substring)."
    )]
    async fn delta_reset(
        &self,
        Parameters(params): Parameters<DeltaResetParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "delta_reset", {
            let cleared = self.ledger.lock().unwrap().clear(params.pattern.as_deref())
                + self
                    .cmd_ledger
                    .lock()
                    .unwrap()
                    .clear(params.pattern.as_deref())
                + self
                    .content_ledger
                    .lock()
                    .unwrap()
                    .clear(params.pattern.as_deref());
            ok_json(serde_json::json!({ "cleared_entries": cleared, "token_count": 10 }))
        })
    }

    #[tool(
        description = "Discover t0k3n-mcp tools. No args: category names only. With category: tool names + one-line descriptions. Pass \"all\" for the full catalog. Categories: file/git/schema/web/notebook/test/log/text/memory/task/session/analysis/cmd/debug."
    )]
    async fn help(
        &self,
        Parameters(params): Parameters<HelpParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "help", { ok_json(help(params)) })
    }

    #[tool(
        description = "Returns server diagnostics: version, root path, DB status, and the full list of registered tools. Call this to confirm t0k3n-mcp is active and all tools are registered correctly."
    )]
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
            let mut tools: Vec<String> =
                self.tool_router.map.keys().map(|k| k.to_string()).collect();
            tools.sort();
            let ledger_git_head = self
                .content_ledger
                .lock()
                .ok()
                .and_then(|l| l.git_head().map(|s| s.to_string()));
            ok_json(serde_json::json!({
                "ok": true,
                "version": env!("CARGO_PKG_VERSION"),
                "root": self.root.display().to_string(),
                "root_configured": self.config.root_configured,
                "db_status": db_status,
                "tool_count": tools.len(),
                "tools": tools,
                "diagnostics_enabled": self.config.diagnostics_enabled,
                "writes_enabled": self.config.writes_enabled,
                "commands_enabled": self.config.commands_enabled,
                "tool_categories": self.config.tool_categories,
                "content_ledger_git_head": ledger_git_head,
                "timestamp_unix": timestamp_unix,
                "dashboard": self.dashboard.is_some(),
            }))
        })
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for T0k3nServer {
    fn get_info(&self) -> ServerInfo {
        // Counts are computed, never hardcoded: a stale "91 tools" in the very
        // instructions that tell the agent to trust this server erodes that trust.
        let tool_count = self.tool_router.map.len();
        let category_count = tools::help::catalog().len();
        let mut instructions = format!(
            "T0K3N-MCP is active ({tool_count} tools across {category_count} categories). \
             Use t0k3n-mcp tools \
             instead of built-in Read/Grep/Glob for all file, web, code-analysis, and \
             memory operations.\n\
             \n\
             RULES:\n\
             1. NEVER read whole files with built-in Read/Grep/Glob — on average 87% of a \
             full-file read is content you never use. Read structure first, then extract \
             only the parts you need.\n\
             2. For code: read_code_skeleton first, then read_code_body for just the symbols \
             you need (zoom: skeleton/sketch/body/auto). The same outline-then-extract \
             pattern exists for markdown, JSON/YAML, CSS, web pages, and notebooks.\n\
             3. Begin a task with project_digest (cached architecture warm-start) and \
             check_budget (token budget + reading strategy). For a specific change, \
             read_context_pack gathers ranked files + symbols + bodies in one call.\n\
             4. Combine multiple read operations into a single batch_read call — one round \
             trip and one response envelope instead of many.\n\
             5. DISCOVER TOOLS WITH help — there are {tool_count} and you will miss the best \
             fit if you \
             guess. Call help() for category names, help(\"<category>\") for that category's \
             tools, or help(\"all\") for the full catalog BEFORE falling back to a generic \
             read, search, or run_command. Categories: file / write / git / schema / web / \
             notebook / test / log / text / memory / task / session / analysis / cmd / debug.\n\
             6. EDITING: prefer surgical writes over rewriting files. patch_symbol (replace a \
             symbol) and rename_symbol are always available; create_file / insert_symbol / \
             delete_symbol / apply_edits are registered only when the server was started with \
             --enable-writes. All support dry_run and return diffs/summaries only — \
             never resend a whole file you are editing.\n\
             7. HEURISTIC RESULTS: read_security_surface, read_dead_code and read_complexity_map \
             are pattern/AST heuristics, not compilers or taint analyzers. Findings from the \
             first two carry a `confidence` field: verify anything below `high` by reading the \
             code before reporting it as a real problem.\n\
             \n\
             DELTA READS: repeat reads return {{unchanged:true}} stubs or unified diffs instead \
             of re-sending identical content. Trust them — the content equals what you already \
             received earlier this session (or, when labeled a cold cache, an unchanged file \
             from a previous session). Each stub carries a `content_sha256` prefix so you can \
             confirm you still hold the referenced content; if you no longer have it \
             (e.g. after compaction), call delta_reset and retry the read.",
        );
        if !self.config.root_configured {
            instructions.push_str(
                "\n\n\
                 NO WORKSPACE ROOT CONFIGURED: this server was started without --root \
                 (or T0K3N_ROOT), so it defaults to its own process working directory, which is \
                 usually NOT the project you want. Pass an absolute `root` argument (e.g. \
                 root: \"D:\\path\\to\\project\") on every tool call to point it at the right \
                 workspace — every tool accepts it even though it is not listed in the tool's \
                 formal input schema. Once `root` is set on the MCP client side (--root / \
                 T0K3N_ROOT), this per-call override is ignored in favor of the configured root.",
            );
        }
        ServerInfo {
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
            instructions: Some(instructions),
            ..Default::default()
        }
    }

    /// Expose the workspace's key files (manifests, READMEs, entry points) as
    /// `t0k3n://` resources for resource-aware clients.
    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let resources = tools::resources::list_workspace_resources(&self.root)
            .into_iter()
            .map(|e| {
                let mut raw = RawResource::new(e.uri, e.name);
                raw.mime_type = Some(e.mime);
                raw.size = Some(e.size);
                raw.no_annotation()
            })
            .collect();
        Ok(ListResourcesResult::with_all_items(resources))
    }

    /// Read one `t0k3n://` resource. The content goes through the cross-session
    /// content ledger's invalidation rules implicitly (it is always the current
    /// file on disk).
    async fn read_resource(
        &self,
        request: ReadResourceRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let path = tools::resources::resolve_uri(&self.root, &request.uri)
            .ok_or_else(|| err(format!("unknown or unsafe resource uri: {}", request.uri)))?;
        let text = std::fs::read_to_string(&path)
            .map_err(|e| err(format!("failed to read {}: {e}", request.uri)))?;
        Ok(ReadResourceResult {
            contents: vec![ResourceContents::text(text, request.uri)],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a server rooted at `dir` with the default capability set, letting the
    /// caller tweak just the flags under test.
    fn test_server(dir: &std::path::Path, tweak: impl FnOnce(&mut ServerConfig)) -> T0k3nServer {
        let mut config = ServerConfig {
            root: dir.to_string_lossy().to_string(),
            root_configured: true,
            ..Default::default()
        };
        tweak(&mut config);
        T0k3nServer::new(config, None)
    }

    #[test]
    fn diagnostics_route_is_opt_in() {
        let tmp = tempfile::tempdir().unwrap();

        let off = test_server(tmp.path(), |_| {});
        assert!(
            !off.tool_router.map.contains_key("read_type_diagnostics"),
            "diagnostics tool must NOT be registered by default"
        );

        let on = test_server(tmp.path(), |c| c.diagnostics_enabled = true);
        assert!(
            on.tool_router.map.contains_key("read_type_diagnostics"),
            "diagnostics tool must be registered with --enable-diagnostics"
        );
    }

    #[test]
    fn command_tools_are_opt_out() {
        let tmp = tempfile::tempdir().unwrap();

        let on = test_server(tmp.path(), |_| {});
        for t in COMMAND_TOOLS {
            assert!(
                on.tool_router.map.contains_key(*t),
                "command tool {t} must be registered by default"
            );
        }

        let off = test_server(tmp.path(), |c| c.commands_enabled = false);
        for t in COMMAND_TOOLS {
            assert!(
                !off.tool_router.map.contains_key(*t),
                "command tool {t} must NOT be registered with --disable-commands"
            );
        }
    }

    #[test]
    fn tool_categories_trim_the_roster() {
        let tmp = tempfile::tempdir().unwrap();
        let full = test_server(tmp.path(), |_| {});
        let trimmed = test_server(tmp.path(), |c| {
            c.tool_categories = Some(vec!["git".to_string()]);
        });

        assert!(trimmed.tool_router.map.len() < full.tool_router.map.len());
        assert!(trimmed.tool_router.map.contains_key("read_git_log"));
        assert!(!trimmed.tool_router.map.contains_key("read_openapi"));
        // Discovery and introspection must survive any profile.
        for t in ALWAYS_KEEP_TOOLS {
            assert!(
                trimmed.tool_router.map.contains_key(*t),
                "{t} must stay registered under a category profile"
            );
        }
    }

    #[test]
    fn short_sha256_is_stable_and_short() {
        assert_eq!(short_sha256("abc"), "ba7816bf8f01");
        assert_eq!(short_sha256("abc").len(), 12);
        assert_ne!(short_sha256("abc"), short_sha256("abd"));
    }

    #[test]
    fn known_tool_categories_are_non_empty_and_lowercase() {
        let cats = known_tool_categories();
        assert!(cats.contains(&"git") && cats.contains(&"file"));
        assert!(cats.iter().all(|c| c == &c.to_ascii_lowercase()));
    }

    #[test]
    fn category_profile_still_honors_the_write_gate() {
        let tmp = tempfile::tempdir().unwrap();
        // "write" selects the write category, but the opt-in gate must still win.
        let server = test_server(tmp.path(), |c| {
            c.tool_categories = Some(vec!["write".to_string()]);
        });
        for t in WRITE_TOOLS {
            assert!(
                !server.tool_router.map.contains_key(*t),
                "{t} must stay gated even when its category is selected"
            );
        }
    }

    /// The tool count is written by hand in the READMEs. It has drifted before, and a
    /// wrong count in the file people read first is the cheapest kind of bug to prevent.
    #[test]
    fn readme_tool_counts_match_the_registry() {
        let expected = REGISTERED_TOOLS.len();
        for (file, heading) in [
            ("README.md", format!("## Tools ({expected} tools)")),
            (
                "README.ja.md",
                format!("## ツール一覧（{expected} ツール）"),
            ),
        ] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(file);
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("could not read {file}: {e}"));
            assert!(
                text.contains(&heading),
                "{file} must contain the heading {heading:?} — \
                 REGISTERED_TOOLS has {expected} entries"
            );
        }
    }

    #[test]
    fn help_catalog_covers_every_registered_tool() {
        use std::collections::HashSet;
        let cataloged: HashSet<&str> = tools::help::catalog()
            .values()
            .flat_map(|entries| entries.iter().map(|e| e.name))
            .collect();
        let missing: Vec<&str> = REGISTERED_TOOLS
            .iter()
            .copied()
            .filter(|t| !cataloged.contains(t))
            .collect();
        assert!(
            missing.is_empty(),
            "help() catalog is missing registered tools: {missing:?}"
        );
    }

    #[test]
    fn write_tools_are_opt_in() {
        let tmp = tempfile::tempdir().unwrap();

        let off = test_server(tmp.path(), |_| {});
        for t in WRITE_TOOLS {
            assert!(
                !off.tool_router.map.contains_key(*t),
                "write tool {t} must NOT be registered by default"
            );
        }
        // patch_symbol / rename_symbol predate the gate and stay always-on.
        assert!(off.tool_router.map.contains_key("patch_symbol"));
        assert!(off.tool_router.map.contains_key("rename_symbol"));

        let on = test_server(tmp.path(), |c| c.writes_enabled = true);
        for t in WRITE_TOOLS {
            assert!(
                on.tool_router.map.contains_key(*t),
                "write tool {t} must be registered with --enable-writes"
            );
        }
    }

    #[test]
    fn zoom_level_auto_degrades_with_budget() {
        // Explicit levels pass through regardless of budget.
        assert_eq!(zoom_level(Some("skeleton"), Some("normal")), "skeleton");
        assert_eq!(zoom_level(Some("sketch"), None), "sketch");
        assert_eq!(zoom_level(Some("body"), Some("critical")), "body");
        // No zoom requested → body.
        assert_eq!(zoom_level(None, Some("critical")), "body");
        // auto follows the budget strategy.
        assert_eq!(zoom_level(Some("auto"), Some("critical")), "skeleton");
        assert_eq!(zoom_level(Some("auto"), Some("aggressive")), "sketch");
        assert_eq!(zoom_level(Some("auto"), Some("conservative")), "body");
        assert_eq!(zoom_level(Some("auto"), Some("normal")), "body");
        // auto with no recorded budget → safe default (body).
        assert_eq!(zoom_level(Some("auto"), None), "body");
        // case-insensitive.
        assert_eq!(zoom_level(Some("AUTO"), Some("critical")), "skeleton");
    }

    #[test]
    fn check_budget_updates_zoom_status() {
        let tmp = tempfile::tempdir().unwrap();
        let server = test_server(tmp.path(), |_| {});
        // Before any check_budget call, auto falls back to body.
        assert_eq!(server.resolve_zoom(Some("auto")), "body");
        // Simulate a critical-budget check_budget result being recorded.
        *server.budget_status.lock().unwrap() = Some("critical".to_string());
        assert_eq!(server.resolve_zoom(Some("auto")), "skeleton");
    }

    #[test]
    fn effective_root_ignores_override_when_configured() {
        let configured = PathBuf::from("/configured/root");
        let mut args: Option<JsonObject> = Some(
            serde_json::json!({ "root": "/override/root", "path": "a.rs" })
                .as_object()
                .unwrap()
                .clone(),
        );
        let resolved = resolve_effective_root(true, &configured, &mut args);
        assert_eq!(resolved, configured);
        // The override key must NOT be consumed when ignored — irrelevant here since the
        // tool's own Parameters<T> never sees a "root" key it would reject anyway, but the
        // unrelated "path" key must survive untouched either way.
        assert_eq!(args.unwrap().get("path").unwrap(), "a.rs");
    }

    #[test]
    fn effective_root_applies_override_when_unconfigured() {
        let configured = PathBuf::from("/configured/root");
        let mut args: Option<JsonObject> = Some(
            serde_json::json!({ "root": "/override/root", "path": "a.rs" })
                .as_object()
                .unwrap()
                .clone(),
        );
        let resolved = resolve_effective_root(false, &configured, &mut args);
        assert_eq!(resolved, PathBuf::from("/override/root"));
        // "root" must be popped out so it never reaches Parameters<T> deserialization.
        let remaining = args.unwrap();
        assert!(!remaining.contains_key("root"));
        assert_eq!(remaining.get("path").unwrap(), "a.rs");
    }

    #[test]
    fn effective_root_falls_back_when_unconfigured_and_no_override_given() {
        let configured = PathBuf::from("/configured/root");
        let mut args: Option<JsonObject> = Some(
            serde_json::json!({ "path": "a.rs" })
                .as_object()
                .unwrap()
                .clone(),
        );
        let resolved = resolve_effective_root(false, &configured, &mut args);
        assert_eq!(resolved, configured);
    }
}
