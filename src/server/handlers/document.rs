//! `convert_document` — the one tool gated behind the `documents` Cargo feature.
//!
//! It lives in its own module (rather than alongside the other `web` tools) because
//! `#[tool_router]` builds its route list from the tokens it is handed, before `cfg`
//! stripping: a `#[cfg]` on an individual handler would still be routed. Gating the
//! whole module — and its router — is what actually removes the tool.
//!
//! Category-wise this still belongs to `web` in `help()`.

use crate::server::*;

#[tool_router(router = document_router, vis = "pub(crate)")]
impl T0k3nServer {
    #[tool(
        description = "Convert a PDF or DOCX to Markdown, return TOC and tmp_path. Use read_markdown_section(tmp_path) to read sections."
    )]
    async fn convert_document(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ConvertDocumentParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "convert_document", {
            let result = convert_document(&root, params).map_err(err)?;
            ok_json(
                serde_json::json!({ "toc": result.toc, "tmp_path": result.tmp_path, "token_count": result.token_count }),
            )
        })
    }
}
