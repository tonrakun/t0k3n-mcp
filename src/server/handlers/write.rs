//! Mutating write tool handlers — the `write` category of `help()`.
//!
//! Registered as `write_router` and merged into the server's router in
//! [`super::tool_router`].

use crate::server::*;

#[tool_router(router = write_router, vis = "pub(crate)")]
impl T0k3nServer {
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
}
