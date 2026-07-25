//! Jupyter notebook tool handlers — the `notebook` category of `help()`.
//!
//! Registered as `notebook_router` and merged into the server's router in
//! [`super::tool_router`].

use crate::server::*;

#[tool_router(router = notebook_router, vis = "pub(crate)")]
impl T0k3nServer {
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
}
