//! Git tool handlers — the `git` category of `help()`.
//!
//! Registered as `git_router` and merged into the server's router in
//! [`super::tool_router`].

use crate::server::*;

#[tool_router(router = git_router, vis = "pub(crate)")]
impl T0k3nServer {
    #[tool(
        description = "Get compressed git diff. Defaults to all uncommitted changes vs HEAD. Use stat_only for a quick file-level summary. zoom mirrors read_code: 'body' (full diff), 'sketch' (file + hunk headers only), 'skeleton' (per-file × enclosing-symbol +/- line counts, no diff text), or 'auto' (follows the latest check_budget strategy). Apply the structure-first read to change itself: skeleton to map a large diff, then body on the suspicious files."
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
        description = "Get per-line blame (author + date) for a specific line range in a file. Use start_line/end_line from read_code to target a function body."
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
}
