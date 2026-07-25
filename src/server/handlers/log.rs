//! Log and diagnostics tool handlers — the `log` category of `help()`.
//!
//! Registered as `log_router` and merged into the server's router in
//! [`super::tool_router`].

use crate::server::*;

#[tool_router(router = log_router, vis = "pub(crate)")]
impl T0k3nServer {
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
}
