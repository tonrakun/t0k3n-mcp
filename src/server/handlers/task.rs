//! Task tracking tool handlers — the `task` category of `help()`.
//!
//! Registered as `task_router` and merged into the server's router in
//! [`super::tool_router`].

use crate::server::*;

#[tool_router(router = task_router, vis = "pub(crate)")]
impl T0k3nServer {
    #[tool(
        description = "Create a task with title, description, status (pending/in_progress/done/cancelled), priority, tags."
    )]
    async fn task_create(
        &self,
        Parameters(params): Parameters<TaskCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "task_create", {
            let db = lock_or_recover(&self.db);
            ok_json(task_create(&db, params).map_err(err)?)
        })
    }

    #[tool(description = "Get a task by ID.")]
    async fn task_get(
        &self,
        Parameters(params): Parameters<TaskGetParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "task_get", {
            let db = lock_or_recover(&self.db);
            ok_json(task_get(&db, params).map_err(err)?)
        })
    }

    #[tool(description = "Update a task's fields. Only provided fields are updated.")]
    async fn task_update(
        &self,
        Parameters(params): Parameters<TaskUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "task_update", {
            let db = lock_or_recover(&self.db);
            ok_json(task_update(&db, params).map_err(err)?)
        })
    }

    #[tool(description = "List tasks, optionally filtered by status or tag.")]
    async fn task_list(
        &self,
        Parameters(params): Parameters<TaskListParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "task_list", {
            let db = lock_or_recover(&self.db);
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
            let db = lock_or_recover(&self.db);
            ok_text(task_delete(&db, params).map_err(err)?)
        })
    }
}
