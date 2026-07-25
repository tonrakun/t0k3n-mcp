//! Introspection tool handlers — the `debug` category of `help()`.
//!
//! Registered as `debug_router` and merged into the server's router in
//! [`super::tool_router`].

use crate::server::*;

#[tool_router(router = debug_router, vis = "pub(crate)")]
impl T0k3nServer {
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
                // Tools a slim (--no-default-features) build omits at compile time,
                // as opposed to the runtime capability gates above.
                "compiled_out_tools": unavailable_tools(),
                "content_ledger_git_head": ledger_git_head,
                "timestamp_unix": timestamp_unix,
                "dashboard": self.dashboard.is_some(),
            }))
        })
    }
}
