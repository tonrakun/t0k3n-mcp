pub(crate) use std::collections::HashMap;
pub(crate) use std::path::{Path, PathBuf};
pub(crate) use std::sync::{Arc, Mutex};
pub(crate) use std::time::Instant;

pub(crate) use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::tool::{FromToolCallContextPart, Parameters, ToolCallContext, ToolRouter},
    model::*,
    service::{RequestContext, RoleServer},
    tool, tool_handler, tool_router,
};
pub(crate) use serde::Serialize;

pub(crate) use crate::dashboard::DashboardState;

mod db;
mod schema_slim;
pub mod tools;

use db::Database;
pub(crate) use tools::render::OutputFormat;
pub(crate) use tools::{
    api_surface::{ReadApiSurfaceParams, read_api_surface},
    audit::{ReadDependencyAuditParams, read_dependency_audit},
    batch::{BatchReadParams, batch_read},
    checkpoint::{EditCheckpointParams, RollbackParams, edit_checkpoint, rollback},
    ci::{ReadCiPipelineParams, read_ci_pipeline},
    cmd::{CmdLedger, RunCommandParams, run_command},
    code::{
        ReadCallGraphParams, ReadCodeBodyParams, ReadCodeParams, ReadCodeSkeletonParams,
        ReadInterfaceConformanceParams, ReadSymbolUsagesParams, ReadTypeSkeletonParams,
        read_call_graph, read_code_body, read_code_skeleton, read_interface_conformance,
        read_symbol_usages, read_type_skeleton,
    },
    complexity::{ReadComplexityMapParams, read_complexity_map},
    config_write::{SetConfigValueParams, set_config_value},
    context_pack::{ReadContextPackParams, read_context_pack},
    coverage::{ReadTestCoverageParams, read_test_coverage},
    css::{
        ReadCssBodyParams, ReadCssParams, ReadCssSkeletonParams, read_css_body, read_css_skeleton,
    },
    db_schema::{
        ReadDbParams, ReadDbSchemaParams, ReadDbTableParams, read_db_schema, read_db_table,
    },
    dead_code::{ReadDeadCodeParams, read_dead_code},
    delta::{ContentDedup, ContentLedger, Delta, DeltaResetParams, ReadLedger},
    deps::{ReadCodeDepsParams, read_code_deps},
    diagnostics::{ReadTypeDiagnosticsParams, read_type_diagnostics},
    diff_schemas::{DiffSchemasParams, diff_schemas},
    digest::{ProjectDigestParams, project_digest},
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
        ReadGraphqlParams, ReadGraphqlSchemaParams, ReadGraphqlTypeParams, read_graphql_schema,
        read_graphql_type,
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
        ReadNotebookCellParams, ReadNotebookCellsParams, ReadNotebookParams, read_notebook_cell,
        read_notebook_cells,
    },
    openapi::{ReadOpenApiParams, read_openapi},
    outline::{ReadFileOutlineParams, read_file_outline},
    ownership::{ReadCodeOwnershipParams, read_code_ownership},
    patch::{PatchSymbolParams, patch_symbol},
    pr_context::{ReadPrContextParams, read_pr_context},
    proto::{
        ReadProtoParams, ReadProtoSchemaParams, ReadProtoTypeParams, read_proto_schema,
        read_proto_type,
    },
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

#[cfg(feature = "documents")]
pub(crate) use tools::document::{ConvertDocumentParams, convert_document};

pub const REGISTERED_TOOLS: &[&str] = &[
    // File reading
    "read_directory_tree",
    "read_markdown_toc",
    "read_markdown_section",
    "search_file",
    "read_json_yaml_keys",
    "read_json_yaml_value",
    "read_code",
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
    "read_db",
    "read_css",
    "read_graphql",
    "read_proto",
    "read_openapi",
    "read_env_schema",
    "read_package_manifest",
    "read_ci_pipeline",
    "read_workspace_stats",
    "read_interface_conformance",
    "batch_read",
    // Notebook
    "read_notebook",
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

/// `convert_document` is compiled only with the `documents` feature (on by default).
/// Listed here so the registry-vs-router guard test can account for a slim build.
pub const DOCUMENT_TOOLS: &[&str] = &["convert_document"];

/// Why a declared tool will not be served under `config`, or `None` when it will be.
///
/// `REGISTERED_TOOLS` is the declared catalog, not the live router: a slim build
/// compiles some tools out, capability flags gate others, and `--tools` trims the
/// rest. `--list-tools` has to answer "what does *this* invocation serve?" before a
/// server exists, so the rules live here; `T0k3nServer::new` shares the roster half
/// via `roster_for`, and `list_tools_agrees_with_the_router` pins the two together.
pub fn tool_exclusion_reason(tool: &str, config: &ServerConfig) -> Option<String> {
    if !cfg!(feature = "documents") && DOCUMENT_TOOLS.contains(&tool) {
        return Some("not in this build — needs the `documents` feature".to_string());
    }
    if WRITE_TOOLS.contains(&tool) && !config.writes_enabled {
        return Some("opt-in — needs --enable-writes".to_string());
    }
    if tool == "read_type_diagnostics" && !config.diagnostics_enabled {
        return Some("opt-in — needs --enable-diagnostics".to_string());
    }
    if COMMAND_TOOLS.contains(&tool) && !config.commands_enabled {
        return Some("off — --disable-commands".to_string());
    }
    match (roster_for(config), config.tool_categories.as_ref()) {
        (Some(keep), Some(cats)) if !keep.contains(tool) => {
            Some(format!("not in --tools {}", cats.join(",")))
        }
        _ => None,
    }
}

/// Tools that cannot be served by this binary at all, whatever the flags.
pub fn unavailable_tools() -> Vec<&'static str> {
    if cfg!(feature = "documents") {
        Vec::new()
    } else {
        DOCUMENT_TOOLS.to_vec()
    }
}

/// Tools that stay registered under every category profile: without them the agent
/// cannot discover what else it has or report its own configuration.
const ALWAYS_KEEP_TOOLS: &[&str] = &["help", "debug_info"];

/// Named bundles of categories accepted by `--tools` alongside bare category names.
/// Choosing a roster is the single biggest lever a user has over schema cost, and
/// asking them to know which of 15 categories matter is a poor way to expose it.
///
/// `core` is the everyday code-reading set: read structure, read history, manage
/// the token budget. Everything left out (schema DSLs, notebooks, databases,
/// sessions, tasks, memory) is real work for the sessions that need it and dead
/// schema weight for the sessions that do not.
pub const TOOL_PROFILES: &[(&str, &[&str])] = &[("core", &["file", "git", "text", "debug"])];

/// Expand one `--tools` token into the help() categories it selects. A profile
/// name expands to several; anything else is passed through as a category name
/// and validated by the caller.
fn expand_profile(token: &str) -> Vec<String> {
    TOOL_PROFILES
        .iter()
        .find(|(name, _)| *name == token)
        .map(|(_, cats)| cats.iter().map(|c| (*c).to_string()).collect())
        .unwrap_or_else(|| vec![token.to_string()])
}

/// Resolve a list of help() category names (or profile names) to the set of tool
/// names to keep. Unknown categories are ignored (validated and reported at
/// startup instead).
pub(crate) fn tools_in_categories(
    categories: &[String],
) -> std::collections::HashSet<&'static str> {
    let catalog = tools::help::catalog();
    let mut keep: std::collections::HashSet<&'static str> =
        ALWAYS_KEEP_TOOLS.iter().copied().collect();
    for token in categories {
        for cat in expand_profile(token.trim().to_ascii_lowercase().as_str()) {
            if let Some(entries) = catalog.get(cat.as_str()) {
                keep.extend(entries.iter().map(|e| e.name));
            }
        }
    }
    keep
}

/// The tool names a `--tools` roster admits, or `None` when no roster is set.
///
/// An explicit capability opt-in is itself a request for those tools, so it outranks
/// the roster. Without this, `--tools core --enable-writes` registers no write tool at
/// all — `core` does not include the write category — and the flag looks accepted
/// while doing nothing. The opt-out capability (`run_command`) is deliberately not
/// treated this way: not passing `--disable-commands` says nothing about intent, and
/// re-adding it would quietly widen every narrowed roster.
pub(crate) fn roster_for(config: &ServerConfig) -> Option<std::collections::HashSet<&'static str>> {
    let cats = config.tool_categories.as_ref()?;
    let mut keep = tools_in_categories(cats);
    if config.writes_enabled {
        keep.extend(tools_in_categories(&["write".to_string()]));
    }
    if config.diagnostics_enabled {
        keep.insert("read_type_diagnostics");
    }
    Some(keep)
}

/// Names accepted by `--tools`: every help() category (taken from the catalog so
/// the two can never drift apart) plus the named profiles.
pub fn known_tool_categories() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = TOOL_PROFILES.iter().map(|(name, _)| *name).collect();
    names.extend(tools::help::catalog().keys().copied());
    names
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
    pub(crate) db: Arc<Mutex<Database>>,
    pub(crate) web_cache: Arc<Mutex<HashMap<String, String>>>,
    pub(crate) ledger: Arc<Mutex<ReadLedger>>,
    pub(crate) cmd_ledger: Arc<Mutex<CmdLedger>>,
    pub(crate) content_ledger: Arc<Mutex<ContentLedger>>,
    /// Latest check_budget strategy (normal/conservative/aggressive/critical),
    /// used by read_code_body's zoom:auto to pick a detail level.
    pub(crate) budget_status: Arc<Mutex<Option<String>>>,
    pub(crate) tool_router: ToolRouter<Self>,
    pub(crate) config: ServerConfig,
    pub dashboard: Option<Arc<DashboardState>>,
}

pub(crate) fn err(msg: impl std::fmt::Display) -> McpError {
    McpError::internal_error(msg.to_string(), None)
}

/// Short content digest published with delta stubs so a caller can verify it still
/// holds the content the stub refers to. 12 hex chars is ample for that check and
/// costs a handful of tokens.
pub(crate) fn short_sha256(content: &str) -> String {
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
pub(crate) struct EffectiveRoot(pub(crate) PathBuf);

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
pub(crate) fn zoom_level(requested: Option<&str>, status: Option<&str>) -> &'static str {
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
pub(crate) fn file_mtime(root: &std::path::Path, rel: &str) -> Option<u64> {
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

pub(crate) fn output_format() -> OutputFormat {
    *OUTPUT_FORMAT.get().unwrap_or(&OutputFormat::Compact)
}

pub(crate) fn ok_json<T: Serialize>(v: T) -> Result<CallToolResult, McpError> {
    let s = match output_format() {
        OutputFormat::Json => serde_json::to_string_pretty(&v).map_err(err)?,
        OutputFormat::Compact => {
            let value = serde_json::to_value(&v).map_err(err)?;
            tools::render::to_compact_text(&value)
        }
    };
    Ok(CallToolResult::success(vec![Content::text(s)]))
}

pub(crate) fn ok_text(s: String) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![Content::text(s)]))
}

/// Pull `token_count` out of a tool response rendered as JSON or compact text.
pub(crate) fn extract_token_count(text: &str) -> Option<u64> {
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
pub(crate) fn delta_key<P: Serialize>(tool: &str, params: &P) -> String {
    format!(
        "{tool}:{}",
        serde_json::to_string(params).unwrap_or_default()
    )
}

/// Lock a mutex, recovering from poisoning instead of panicking.
///
/// Now that tool panics are caught rather than fatal, a poisoned lock would
/// otherwise turn one panic into a permanently broken server: every later call
/// touching that mutex would panic on `unwrap()`. The guarded state here is
/// caches and ledgers — stale entries are recoverable, an unusable server is not.
pub(crate) fn lock_or_recover<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| {
        tracing::warn!("recovering a poisoned lock after an earlier panic");
        poisoned.into_inner()
    })
}

/// Turn a caught panic payload into a tool error.
///
/// The server is a long-lived stdio process shared by a whole editing session: a
/// panic in one tool (a slice out of bounds in a parser, a poisoned lock, an
/// `unwrap` on unexpected input) must not take the session down with it.
pub(crate) fn panic_to_error(tool: &str, payload: Box<dyn std::any::Any + Send>) -> McpError {
    let detail = payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_string());
    tracing::error!("tool {tool} panicked: {detail}");
    McpError::internal_error(
        format!(
            "{tool} panicked: {detail}. This is a bug in t0k3n — the server is still \
             running, so other tools remain usable. Please report it with the input \
             that triggered it."
        ),
        None,
    )
}

/// Wraps a tool body: captures timing, isolates panics, records to dashboard on completion.
/// The inner async block contains the `?` operators so early-exit errors are still recorded.
macro_rules! instrument {
    ($self:expr, $name:literal, $body:block) => {{
        let __t = Instant::now();
        // AssertUnwindSafe: the shared state behind the panic boundary is either
        // immutable or a Mutex, and every lock here recovers from poisoning
        // (`lock_or_recover`), so a caught panic cannot leave a locked-out server.
        let __fut = std::panic::AssertUnwindSafe(async $body);
        let __r: Result<CallToolResult, McpError> =
            match futures_util::FutureExt::catch_unwind(__fut).await {
                Ok(r) => r,
                Err(payload) => Err(panic_to_error($name, payload)),
            };
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

// `instrument!` is textually scoped: the handler modules can only use it because
// `mod handlers;` is declared *after* the macro_rules! definition above. Do not
// move this declaration earlier in the file.
mod handlers;

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
        // `roster_for` folds the explicit capability opt-ins back in, so a narrow
        // roster cannot silently cancel --enable-writes / --enable-diagnostics.
        if let Some(keep) = roster_for(&config) {
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
        // The client re-sends every remaining schema on every request, so trim the
        // schemars boilerplate ($schema, the struct-name title) that no MCP client
        // reads. Done after the gates so unregistered tools cost nothing at all.
        for route in tool_router.map.values_mut() {
            schema_slim::slim_schema(std::sync::Arc::make_mut(&mut route.attr.input_schema));
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

    fn resolve_zoom(&self, requested: Option<&str>) -> &'static str {
        let status = self.budget_status.lock().ok().and_then(|s| s.clone());
        zoom_level(requested, status.as_deref())
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
             2. For code: read_code without ids first, then read_code with ids for just the symbols \
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
    fn lock_or_recover_survives_a_poisoned_mutex() {
        let m = Arc::new(Mutex::new(5u32));
        let m2 = m.clone();
        // Poison the mutex from another thread.
        let _ = std::thread::spawn(move || {
            let _guard = m2.lock().unwrap();
            panic!("poison it");
        })
        .join();
        assert!(m.lock().is_err(), "the mutex should now be poisoned");
        assert_eq!(*lock_or_recover(&m), 5, "state must still be readable");
    }

    #[tokio::test]
    async fn a_panicking_tool_body_becomes_an_error_not_an_abort() {
        // Mirrors what instrument! does around every tool body.
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {})); // keep the test output clean
        let fut = std::panic::AssertUnwindSafe(async {
            panic!("index out of bounds");
            #[allow(unreachable_code)]
            Ok::<CallToolResult, McpError>(CallToolResult::success(vec![]))
        });
        let result = match futures_util::FutureExt::catch_unwind(fut).await {
            Ok(r) => r,
            Err(payload) => Err(panic_to_error("read_code", payload)),
        };
        std::panic::set_hook(previous);

        let message = result
            .expect_err("a panic must surface as an error")
            .message;
        assert!(message.contains("read_code panicked"));
        assert!(message.contains("index out of bounds"));
    }

    #[test]
    fn short_sha256_is_stable_and_short() {
        assert_eq!(short_sha256("abc"), "ba7816bf8f01");
        assert_eq!(short_sha256("abc").len(), 12);
        assert_ne!(short_sha256("abc"), short_sha256("abd"));
    }

    #[test]
    fn tool_exclusion_reason_explains_gated_and_compiled_out_tools() {
        let default = ServerConfig::default();
        // Always-on tools carry no note.
        assert_eq!(tool_exclusion_reason("read_code", &default), None);
        assert_eq!(tool_exclusion_reason("run_command", &default), None);
        // Capability-gated tools do — until the flag that ungates them is given.
        assert!(tool_exclusion_reason("create_file", &default).is_some());
        assert!(tool_exclusion_reason("read_type_diagnostics", &default).is_some());
        let opened = ServerConfig {
            writes_enabled: true,
            diagnostics_enabled: true,
            commands_enabled: false,
            ..Default::default()
        };
        assert_eq!(tool_exclusion_reason("create_file", &opened), None);
        assert_eq!(
            tool_exclusion_reason("read_type_diagnostics", &opened),
            None
        );
        assert!(tool_exclusion_reason("run_command", &opened).is_some());
        // A roster excludes what it does not select, and says which roster did it.
        let core = ServerConfig {
            tool_categories: Some(vec!["core".to_string()]),
            ..Default::default()
        };
        assert_eq!(tool_exclusion_reason("read_git_log", &core), None);
        assert_eq!(
            tool_exclusion_reason("read_notebook", &core).as_deref(),
            Some("not in --tools core")
        );
        // The document tools depend on how this test binary was built.
        assert_eq!(
            tool_exclusion_reason("convert_document", &default).is_some(),
            !cfg!(feature = "documents")
        );
        assert_eq!(
            unavailable_tools().is_empty(),
            cfg!(feature = "documents"),
            "a slim build must report its compiled-out tools"
        );
        // Every note must describe a tool that is actually declared.
        for t in unavailable_tools() {
            assert!(REGISTERED_TOOLS.contains(&t));
        }
    }

    /// `--list-tools` reports what an invocation will serve without building a server,
    /// so its answer has to be the router's answer. Drift here is invisible: the list
    /// stays plausible while describing a roster nobody is running.
    #[test]
    fn list_tools_agrees_with_the_router() {
        let tmp = tempfile::tempdir().unwrap();
        let configs = [
            ServerConfig::default(),
            ServerConfig {
                tool_categories: Some(vec!["core".to_string()]),
                ..Default::default()
            },
            ServerConfig {
                tool_categories: Some(vec!["core".to_string()]),
                writes_enabled: true,
                diagnostics_enabled: true,
                ..Default::default()
            },
            ServerConfig {
                tool_categories: Some(vec!["git".to_string(), "write".to_string()]),
                commands_enabled: false,
                ..Default::default()
            },
        ];
        for config in configs {
            let expected: Vec<&str> = REGISTERED_TOOLS
                .iter()
                .copied()
                .filter(|t| tool_exclusion_reason(t, &config).is_none())
                .collect();
            let server = test_server(tmp.path(), |c| {
                c.tool_categories = config.tool_categories.clone();
                c.writes_enabled = config.writes_enabled;
                c.diagnostics_enabled = config.diagnostics_enabled;
                c.commands_enabled = config.commands_enabled;
            });
            for t in &expected {
                assert!(
                    server.tool_router.map.contains_key(*t),
                    "--list-tools promises {t} under {:?} but the router drops it",
                    config.tool_categories
                );
            }
            assert_eq!(
                server.tool_router.map.len(),
                expected.len(),
                "--list-tools and the router disagree on the roster size under {:?}",
                config.tool_categories
            );
        }
    }

    /// `--tools core --enable-writes` used to register no write tool at all: the
    /// profile has no write category, and the capability gate cannot re-add what the
    /// roster already removed. The flag looked accepted while doing nothing.
    #[test]
    fn capability_opt_ins_outrank_a_narrow_roster() {
        let tmp = tempfile::tempdir().unwrap();
        let server = test_server(tmp.path(), |c| {
            c.tool_categories = Some(vec!["core".to_string()]);
            c.writes_enabled = true;
            c.diagnostics_enabled = true;
        });
        for t in WRITE_TOOLS {
            assert!(
                server.tool_router.map.contains_key(*t),
                "{t} must survive a narrow roster when --enable-writes is given"
            );
        }
        // patch_symbol / rename_symbol live in the same category and come back with it.
        assert!(server.tool_router.map.contains_key("patch_symbol"));
        assert!(
            server.tool_router.map.contains_key("read_type_diagnostics"),
            "--enable-diagnostics must survive a roster that omits its category"
        );
        // The roster still trims everything nobody opted into.
        assert!(!server.tool_router.map.contains_key("read_notebook"));

        // Without the opt-in, the roster keeps trimming as before.
        let plain = test_server(tmp.path(), |c| {
            c.tool_categories = Some(vec!["core".to_string()]);
        });
        assert!(!plain.tool_router.map.contains_key("patch_symbol"));
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

    /// A profile that silently expands to nothing would look like a working flag
    /// while serving only help/debug_info, so pin down what `core` actually selects.
    #[test]
    fn core_profile_expands_to_the_everyday_reading_roster() {
        let tmp = tempfile::tempdir().unwrap();
        let full = test_server(tmp.path(), |_| {});
        let core = test_server(tmp.path(), |c| {
            c.tool_categories = Some(vec!["core".to_string()]);
        });

        assert!(
            core.tool_router.map.len() < full.tool_router.map.len(),
            "a profile that trims nothing is not a profile"
        );
        // One representative per bundled category.
        for t in [
            "read_code",
            "read_git_log",
            "check_budget",
            "help",
            "debug_info",
        ] {
            assert!(
                core.tool_router.map.contains_key(t),
                "{t} belongs to the core profile"
            );
        }
        // Categories deliberately left out of core.
        for t in ["read_openapi", "read_notebook", "task_create"] {
            assert!(
                !core.tool_router.map.contains_key(t),
                "{t} is outside core and must not be registered"
            );
        }
    }

    /// Every category a profile names must exist in the catalog, or the profile
    /// quietly shrinks when a category is renamed.
    #[test]
    fn every_profile_names_only_real_categories() {
        let catalog = tools::help::catalog();
        for (profile, categories) in TOOL_PROFILES {
            for cat in *categories {
                assert!(
                    catalog.contains_key(*cat),
                    "profile `{profile}` names `{cat}`, which is not a help() category"
                );
            }
        }
    }

    /// `--tools` validates its input against this list, so a profile missing from
    /// it would be rejected at startup as an unknown category.
    #[test]
    fn known_tool_categories_advertises_the_profiles() {
        let known = known_tool_categories();
        for (profile, _) in TOOL_PROFILES {
            assert!(
                known.contains(profile),
                "profile `{profile}` must be accepted by --tools"
            );
        }
        assert!(known.contains(&"git"), "bare categories still work");
    }

    /// Schema boilerplate is invisible in normal use — nothing breaks, the context
    /// window just quietly shrinks — so it can creep back in unnoticed when a
    /// dependency changes how it derives schemas. Assert against the schemas the
    /// server actually serves, not against `slim_schema` in isolation.
    #[test]
    fn served_schemas_carry_no_schemars_boilerplate() {
        let tmp = tempfile::tempdir().unwrap();
        let server = test_server(tmp.path(), |c| {
            c.diagnostics_enabled = true;
            c.writes_enabled = true;
        });

        for (name, route) in &server.tool_router.map {
            let rendered = serde_json::to_string(&route.attr.input_schema).unwrap();
            assert!(
                !rendered.contains("\"$schema\""),
                "{name} still advertises a $schema key"
            );
            // `"title":` as a schema keyword always appears with a string value at
            // keyword position; a `title` *argument* appears as a properties key,
            // which serializes the same way. Check the keyword form specifically by
            // walking the schema instead of substring-matching the whole blob.
            let value: serde_json::Value = serde_json::from_str(&rendered).unwrap();
            assert!(
                value.get("title").is_none(),
                "{name} still advertises a struct-name title"
            );
        }

        // task_create takes a literal `title` argument: the trim must not have
        // stripped a real parameter while removing the keyword of the same name.
        let task_create = &server.tool_router.map["task_create"];
        assert!(
            task_create.attr.input_schema["properties"]
                .get("title")
                .is_some(),
            "the `title` argument of task_create must survive the trim"
        );
    }

    /// The router is assembled by merging one router per category module. A category
    /// module that is added but never merged in `handlers::tool_router` would silently
    /// unregister its tools, so assert the merged router matches the registry exactly.
    #[test]
    fn merged_router_registers_exactly_the_declared_tools() {
        use std::collections::HashSet;
        // Everything enabled, so the capability gates do not mask a missing merge.
        let tmp = tempfile::tempdir().unwrap();
        let server = test_server(tmp.path(), |c| {
            c.diagnostics_enabled = true;
            c.writes_enabled = true;
        });
        let registered: HashSet<&str> = server.tool_router.map.keys().map(|k| k.as_ref()).collect();
        let mut declared: HashSet<&str> = REGISTERED_TOOLS.iter().copied().collect();
        // A slim build (--no-default-features) compiles the document tools out entirely.
        if !cfg!(feature = "documents") {
            for t in DOCUMENT_TOOLS {
                declared.remove(*t);
            }
        }

        let missing: Vec<&&str> = declared.difference(&registered).collect();
        let unexpected: Vec<&&str> = registered.difference(&declared).collect();
        assert!(
            missing.is_empty(),
            "declared in REGISTERED_TOOLS but not merged into the router \
             (is the category module merged in handlers::tool_router?): {missing:?}"
        );
        assert!(
            unexpected.is_empty(),
            "in the router but missing from REGISTERED_TOOLS: {unexpected:?}"
        );
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
