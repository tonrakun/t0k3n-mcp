//! Web fetching tool handlers — part of the `web` category of `help()`.
//!
//! `convert_document`, the third tool in that category, lives in [`super::document`]
//! because it is gated behind the `documents` Cargo feature.
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
}
