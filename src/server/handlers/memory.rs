//! Persistent memory tool handlers — the `memory` category of `help()`.
//!
//! Registered as `memory_router` and merged into the server's router in
//! [`super::tool_router`].

use crate::server::*;

#[tool_router(router = memory_router, vis = "pub(crate)")]
impl T0k3nServer {
    #[tool(description = "Save a key-value memory to persistent storage (.t0k3n/t0k3n.db).")]
    async fn memory_save(
        &self,
        Parameters(params): Parameters<MemorySaveParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "memory_save", {
            let db = lock_or_recover(&self.db);
            ok_text(memory_save(&db, params).map_err(err)?)
        })
    }

    #[tool(description = "Get a memory entry by key.")]
    async fn memory_get(
        &self,
        Parameters(params): Parameters<MemoryGetParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "memory_get", {
            let db = lock_or_recover(&self.db);
            ok_json(memory_get(&db, params).map_err(err)?)
        })
    }

    #[tool(description = "List all memories, optionally filtered by tag or keyword search.")]
    async fn memory_list(
        &self,
        Parameters(params): Parameters<MemoryListParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "memory_list", {
            let db = lock_or_recover(&self.db);
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
            let db = lock_or_recover(&self.db);
            ok_text(memory_delete(&db, params).map_err(err)?)
        })
    }
}
