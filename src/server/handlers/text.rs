//! Text and token budget tool handlers — the `text` category of `help()`.
//!
//! Registered as `text_router` and merged into the server's router in
//! [`super::tool_router`].

use crate::server::*;

#[tool_router(router = text_router, vis = "pub(crate)")]
impl T0k3nServer {
    #[tool(
        description = "Compress text by removing excessive whitespace and noise. Returns compressed text with token stats."
    )]
    async fn compress_text(
        &self,
        Parameters(params): Parameters<CompressTextParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "compress_text", { ok_json(compress_text(params)) })
    }

    #[tool(description = "Count approximate tokens, characters, and lines in a text.")]
    async fn count_tokens(
        &self,
        Parameters(params): Parameters<CountTokensParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "count_tokens", { ok_json(count_tokens(params)) })
    }

    #[tool(
        description = "Check token budget and get reading strategy (normal/conservative/aggressive/critical)."
    )]
    async fn check_budget(
        &self,
        Parameters(params): Parameters<CheckBudgetParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "check_budget", {
            let result = check_budget(params);
            // Remember the strategy so read_code_body's zoom:auto can use it.
            if let Ok(mut s) = self.budget_status.lock() {
                *s = Some(result.strategy.clone());
            }
            ok_json(result)
        })
    }

    #[tool(description = "Summarize conversation text to fit within a token budget.")]
    async fn summarize_conversation(
        &self,
        Parameters(params): Parameters<SummarizeConversationParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "summarize_conversation", {
            ok_json(summarize_conversation(params))
        })
    }

    #[tool(
        description = "Reset the delta ledgers (delta reads, run_command deltas, AND the cross-tool content ledger). After this, read tools return full content and run_command returns full output again instead of 'unchanged'/diff/delta/'already sent' stubs. Call when you no longer have earlier tool output in context (e.g. after conversation compaction). Optional pattern narrows the reset to matching keys (e.g. a file path or command substring)."
    )]
    async fn delta_reset(
        &self,
        Parameters(params): Parameters<DeltaResetParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "delta_reset", {
            let cleared = lock_or_recover(&self.ledger).clear(params.pattern.as_deref())
                + self
                    .cmd_ledger
                    .lock()
                    .unwrap()
                    .clear(params.pattern.as_deref())
                + self
                    .content_ledger
                    .lock()
                    .unwrap()
                    .clear(params.pattern.as_deref());
            ok_json(serde_json::json!({ "cleared_entries": cleared, "token_count": 10 }))
        })
    }
}
