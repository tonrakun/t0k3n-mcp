//! Test tool handlers — the `test` category of `help()`.
//!
//! Registered as `test_router` and merged into the server's router in
//! [`super::tool_router`].

use crate::server::*;

#[tool_router(router = test_router, vis = "pub(crate)")]
impl T0k3nServer {
    #[tool(
        description = "Get test case list from a test file (Jest/pytest/Rust/#[test]/Go/JUnit/RSpec). Returns IDs usable with read_code_body to get test implementations."
    )]
    async fn read_test_skeleton(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadTestSkeletonParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_test_skeleton", {
            let result = read_test_skeleton(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path, "framework": result.framework,
                "tests": result.tests, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Parse test runner output (Jest/Vitest/pytest/cargo test/go test) into a structured summary: pass/fail counts per suite and failure details. Accepts raw text or a file path."
    )]
    async fn read_test_results(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadTestResultsParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_test_results", {
            let result = read_test_results(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "framework": result.framework, "summary": result.summary,
                "suites": result.suites, "failures": result.failures,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Map a coverage report onto code symbols to see which functions are untested (risky to change). Auto-detects lcov (lcov.info / cargo llvm-cov), coverage.py JSON, or cobertura XML. Per-symbol covered/total/pct plus overall_pct. Filter with uncovered_only (pct<100) or threshold. If no report exists, returns report_available:false + a generation hint (safe to call speculatively). Pairs with read_test_results / read_test_skeleton."
    )]
    async fn read_test_coverage(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadTestCoverageParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_test_coverage", {
            let result = read_test_coverage(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "report_available": result.report_available,
                "format": result.format,
                "overall_pct": result.overall_pct,
                "files": result.files,
                "hint": result.hint,
                "token_count": result.token_count,
            }))
        })
    }
}
