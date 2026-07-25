//! Web and document tool handlers — the `web` category of `help()`.
//!
//! Registered as `web_router` and merged into the server's router in
//! [`super::tool_router`].

use crate::server::*;

#[tool_router(router = web_router, vis = "pub(crate)")]
impl T0k3nServer {
    #[tool(
        description = "Fetch a webpage, convert HTML to Markdown, return TOC only. Call read_webpage_section to read specific sections."
    )]
    async fn fetch_webpage(
        &self,
        Parameters(params): Parameters<FetchWebpageParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "fetch_webpage", {
            let cache = self.web_cache.clone();
            let result = fetch_webpage(params, cache).await.map_err(err)?;
            ok_json(
                serde_json::json!({ "toc": result.toc, "token_count": result.token_count, "cached": result.cached }),
            )
        })
    }

    #[tool(
        description = "Get specific sections from a cached webpage by anchor. Call fetch_webpage first."
    )]
    async fn read_webpage_section(
        &self,
        Parameters(params): Parameters<ReadWebpageSectionParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_webpage_section", {
            let cache = self.web_cache.clone();
            let result = read_webpage_section(params, cache).map_err(err)?;
            ok_json(
                serde_json::json!({ "sections": result.sections, "token_count": result.token_count }),
            )
        })
    }

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
