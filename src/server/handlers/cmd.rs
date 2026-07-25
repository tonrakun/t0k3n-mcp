//! Command execution tool handlers — the `cmd` category of `help()`.
//!
//! Registered as `cmd_router` and merged into the server's router in
//! [`super::tool_router`].

use crate::server::*;

#[tool_router(router = cmd_router, vis = "pub(crate)")]
impl T0k3nServer {
    #[tool(
        description = "Execute a shell command and return token-efficient output. On success: last ~30 lines (final summary). On failure: extracted error lines + warning lines + last ~20 lines for context. Use for build tools (cargo, npm, go, make, mvn), test runners (cargo test, pytest, jest), linters (clippy, eslint, flake8), and type checkers (tsc, mypy). Repeat runs of the same command return only the delta: new/resolved/unchanged error and warning counts plus the new lines — unchanged lines equal what you already received. Call delta_reset and rerun for full output."
    )]
    async fn run_command(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<RunCommandParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "run_command", {
            let key = CmdLedger::key(&params.command, params.cwd.as_deref());
            let result = run_command(&root, params).map_err(err)?;
            let delta = self
                .cmd_ledger
                .lock()
                .unwrap()
                .check_and_update(&key, &result);
            match delta {
                None => ok_json(serde_json::json!({
                    "command":     result.command,
                    "exit_code":   result.exit_code,
                    "success":     result.success,
                    "duration_ms": result.duration_ms,
                    "summary":     result.summary,
                    "errors":      result.errors,
                    "warnings":    result.warnings,
                    "token_count": result.token_count,
                })),
                Some(d) => {
                    let repr = format!(
                        "{}\n{}\n{}",
                        d.summary.as_deref().unwrap_or(""),
                        d.new_errors.join("\n"),
                        d.new_warnings.join("\n")
                    );
                    let mut v = serde_json::json!({
                        "command":     result.command,
                        "exit_code":   result.exit_code,
                        "success":     result.success,
                        "duration_ms": result.duration_ms,
                        "delta":       true,
                        "success_changed":   d.success_changed,
                        "errors_new":        d.new_errors,
                        "errors_resolved":   d.resolved_errors,
                        "errors_unchanged":  d.unchanged_errors,
                        "warnings_new":      d.new_warnings,
                        "warnings_resolved": d.resolved_warnings,
                        "warnings_unchanged": d.unchanged_warnings,
                        "note": "Delta vs the previous run of this command this session — unchanged errors/warnings not re-sent. Call delta_reset and rerun for full output.",
                        "token_count": tools::fs::estimate_tokens(&repr),
                    });
                    if let Some(summary) = d.summary {
                        v["summary"] = serde_json::Value::String(summary);
                    }
                    ok_json(v)
                }
            }
        })
    }
}
