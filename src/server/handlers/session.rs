//! Session snapshot tool handlers — the `session` category of `help()`.
//!
//! Registered as `session_router` and merged into the server's router in
//! [`super::tool_router`].

use crate::server::*;

#[tool_router(router = session_router, vis = "pub(crate)")]
impl T0k3nServer {
    #[tool(
        description = "Save a snapshot of work state (arbitrary JSON) for restoration in future sessions."
    )]
    async fn session_snapshot(
        &self,
        Parameters(params): Parameters<SessionSnapshotParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "session_snapshot", {
            let db = lock_or_recover(&self.db);
            ok_json(session_snapshot(&db, params).map_err(err)?)
        })
    }

    #[tool(description = "Restore a previously saved session snapshot by ID.")]
    async fn session_restore(
        &self,
        Parameters(params): Parameters<SessionRestoreParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "session_restore", {
            let db = lock_or_recover(&self.db);
            ok_json(session_restore(&db, params).map_err(err)?)
        })
    }

    #[tool(description = "List saved session snapshots (most recent first).")]
    async fn session_list(
        &self,
        Parameters(params): Parameters<SessionListParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "session_list", {
            let db = lock_or_recover(&self.db);
            let sessions = session_list(&db, params).map_err(err)?;
            let count = sessions.len();
            ok_json(serde_json::json!({ "sessions": sessions, "count": count }))
        })
    }
}
