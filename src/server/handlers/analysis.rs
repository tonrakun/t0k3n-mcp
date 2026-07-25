//! Code analysis tool handlers — the `analysis` category of `help()`.
//!
//! Registered as `analysis_router` and merged into the server's router in
//! [`super::tool_router`].

use crate::server::*;

#[tool_router(router = analysis_router, vis = "pub(crate)")]
impl T0k3nServer {
    #[tool(
        description = "Compute cyclomatic complexity for every function in a file or directory. Returns functions sorted by complexity with risk level (low/medium/high/critical). Use to identify refactoring targets without running a linter."
    )]
    async fn read_complexity_map(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadComplexityMapParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_complexity_map", {
            let result = read_complexity_map(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "entries": result.entries,
                "total_analyzed": result.total_analyzed,
                "high_risk_count": result.high_risk_count,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Find unused symbols (functions, classes, structs) that are defined but never called across the workspace. Works across all tree-sitter supported languages without a compiler or LSP. HEURISTIC name-based matching: trait/interface impls, dynamic dispatch and reflection targets can look unused, so each entry carries a `confidence` — confirm before deleting."
    )]
    async fn read_dead_code(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadDeadCodeParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_dead_code", {
            let result = read_dead_code(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "entries": result.entries,
                "total_symbols_checked": result.total_symbols_checked,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Blast-radius analysis for a refactor: given a symbol name, returns all callers, all files that reference it, and all test files that cover it — in one call. Combines call_graph + symbol_usages + test discovery."
    )]
    async fn read_refactor_impact(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadRefactorImpactParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_refactor_impact", {
            let result = read_refactor_impact(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "symbol": result.symbol,
                "definition_file": result.definition_file,
                "definition_line": result.definition_line,
                "direct_callers": result.direct_callers,
                "direct_callees": result.direct_callees,
                "referenced_in": result.referenced_in,
                "total_references": result.total_references,
                "test_files": result.test_files,
                "blast_radius": result.blast_radius,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Static security surface scan: flags potential injection vectors, XSS sinks, hardcoded secrets, unsafe code, and path traversal patterns. HEURISTIC line-pattern matcher, not taint analysis — every finding carries `severity` (impact if real) AND `confidence` (how likely it is real); verify anything below high confidence by reading the code. Test code is skipped unless include_tests:true; pass min_confidence to cut noise. Categories: injection, xss, secrets, unsafe, path_traversal, all."
    )]
    async fn read_security_surface(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadSecuritySurfaceParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_security_surface", {
            let result = read_security_surface(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "findings": result.findings,
                "total": result.total,
                "by_category": result.by_category,
                "by_severity": result.by_severity,
                "by_confidence": result.by_confidence,
                "note": result.note,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Scan dependencies for known vulnerabilities — the dependency-side counterpart to read_security_surface. Auto-detects the ecosystem (Cargo.toml→cargo audit, package.json→npm audit, pyproject/requirements→pip-audit, go.mod→osv-scanner) and normalizes results to {package, severity, id, affected, patched, title}, sorted by severity. Filter with severity (minimum level) / max_items. If the scanner is not installed, returns scanner_available:false + an install hint (safe to call speculatively)."
    )]
    async fn read_dependency_audit(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadDependencyAuditParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_dependency_audit", {
            let result = read_dependency_audit(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "scanner_available": result.scanner_available,
                "ecosystem": result.ecosystem,
                "vulnerabilities": result.vulnerabilities,
                "hint": result.hint,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Extract only a codebase's public API surface: Rust pub items, TS/JS exports, Python __all__ / non-underscore top-level defs, Go capitalized identifiers. Signatures only (no bodies). Use to understand a library's external boundary or to detect breaking changes (pair with diff_schemas). Scope with path; include_crate_visible:true also lists Rust pub(crate)/pub(super)."
    )]
    async fn read_api_surface(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadApiSurfaceParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_api_surface", {
            let result = read_api_surface(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "api": result.api,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Diff a schema file (OpenAPI, Prisma/SQL, TypeScript types) between two git refs. Returns added/removed/modified endpoints, tables, or types. Auto-detects schema type from file extension."
    )]
    async fn diff_schemas(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<DiffSchemasParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "diff_schemas", {
            let result = diff_schemas(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path,
                "schema_type": result.schema_type,
                "before_ref": result.before_ref,
                "after_ref": result.after_ref,
                "added": result.added,
                "removed": result.removed,
                "modified": result.modified,
                "total_changes": result.total_changes,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Load full PR context in one call: changed files with skeletons, diff stats, related test files, and commit list. Pass branch + base to get everything needed for a code review without multiple round-trips."
    )]
    async fn read_pr_context(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadPrContextParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_pr_context", {
            let result = read_pr_context(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "branch": result.branch,
                "base": result.base,
                "changed_files": result.changed_files,
                "total_files": result.total_files,
                "total_added": result.total_added,
                "total_deleted": result.total_deleted,
                "related_tests": result.related_tests,
                "commits": result.commits,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Static type diagnostics (LSP-equivalent) without running a language server. OPT-IN: this tool is only registered when the server is started with --enable-diagnostics (or T0K3N_ENABLE_DIAGNOSTICS=1), because it spawns the language toolchain. Drives the language's own check-only engine — cargo check (Rust), tsc --noEmit (TypeScript), pyright/mypy (Python), go vet (Go) — and returns a compact, deduplicated list of {file, line, col, severity, code, message}. Auto-detects the language from the manifest/extension; pass `language` to force it, `path` to scope to a file/dir, `severity` (error|warning|hint) as a floor, and `max_items` to cap. If the checker is not installed it returns checker_available:false with an install hint instead of erroring."
    )]
    async fn read_type_diagnostics(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadTypeDiagnosticsParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_type_diagnostics", {
            if !self.config.diagnostics_enabled {
                return ok_json(serde_json::json!({
                    "error": "read_type_diagnostics is disabled. Restart the server with --enable-diagnostics (or set T0K3N_ENABLE_DIAGNOSTICS=1) to use it.",
                    "token_count": 30,
                }));
            }
            let result = read_type_diagnostics(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "language": result.language,
                "checker": result.checker,
                "checker_available": result.checker_available,
                "note": result.note,
                "diagnostics": result.diagnostics,
                "summary": result.summary,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Warm-start project digest: a cached ~2k-token architecture summary (git HEAD, language stats, entry-point files with their top symbols, shallow directory tree) returned in one call. Replaces the repeated tree → stats → skeleton exploration at session start. The cache (.t0k3n/digest.json) auto-invalidates when git HEAD changes; pass refresh:true to rebuild. `dirty` flags an uncommitted working tree (digest may be stale)."
    )]
    async fn project_digest(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ProjectDigestParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "project_digest", {
            let result = project_digest(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "cached": result.cached,
                "dirty": result.dirty,
                "digest": result.digest,
                "token_count": result.token_count,
            }))
        })
    }
}
