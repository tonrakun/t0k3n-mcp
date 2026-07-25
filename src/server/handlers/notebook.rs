//! Jupyter notebook tool handlers — the `notebook` category of `help()`.
//!
//! Registered as `notebook_router` and merged into the server's router in
//! [`super::tool_router`].

use crate::server::*;

#[tool_router(router = notebook_router, vis = "pub(crate)")]
impl T0k3nServer {
    #[tool(
        description = "Read a Jupyter notebook (.ipynb). Omit index to list every cell with its type, a source preview, and its output count; pass an index to get that cell's full source. Set include_outputs=true to also fetch its outputs."
    )]
    async fn read_notebook(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadNotebookParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_notebook", {
            match params.index {
                Some(index) => {
                    let result = read_notebook_cell(
                        &root,
                        ReadNotebookCellParams {
                            path: params.path,
                            index,
                            include_outputs: params.include_outputs,
                        },
                    )
                    .map_err(err)?;
                    ok_json(serde_json::json!({
                        "path": result.path, "index": result.index, "cell_type": result.cell_type,
                        "execution_count": result.execution_count, "source": result.source,
                        "outputs": result.outputs, "token_count": result.token_count,
                    }))
                }
                None => {
                    let result =
                        read_notebook_cells(&root, ReadNotebookCellsParams { path: params.path })
                            .map_err(err)?;
                    ok_json(serde_json::json!({
                        "path": result.path, "nbformat": result.nbformat,
                        "cells": result.cells, "token_count": result.token_count,
                    }))
                }
            }
        })
    }
}
