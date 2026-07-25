//! File and code reading tool handlers — the `file` category of `help()`.
//!
//! Registered as `file_router` and merged into the server's router in
//! [`super::tool_router`].

use crate::server::*;

#[tool_router(router = file_router, vis = "pub(crate)")]
impl T0k3nServer {
    #[tool(
        description = "Get .gitignore-aware directory tree. Use to explore workspace structure before reading files."
    )]
    async fn read_directory_tree(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadDirectoryTreeParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_directory_tree", {
            let key = delta_key("read_directory_tree", &params);
            let result = read_directory_tree(&root, params).map_err(err)?;
            self.ok_delta(
                key,
                serde_json::json!({ "tree": result.tree, "token_count": result.token_count }),
            )
        })
    }

    #[tool(
        description = "Get all headings (TOC) from a Markdown file. Call before read_markdown_section to get anchors."
    )]
    async fn read_markdown_toc(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadMarkdownTocParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_markdown_toc", {
            let key = delta_key("read_markdown_toc", &params);
            let result = read_markdown_toc(&root, params).map_err(err)?;
            self.ok_delta(
                key,
                serde_json::json!({ "toc": result.toc, "token_count": result.token_count }),
            )
        })
    }

    #[tool(
        description = "Get specific sections from a Markdown file by anchor. Call read_markdown_toc first to get anchors."
    )]
    async fn read_markdown_section(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadMarkdownSectionParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_markdown_section", {
            let key = delta_key("read_markdown_section", &params);
            let result = read_markdown_section(&root, params).map_err(err)?;
            self.ok_delta(key, serde_json::json!({ "sections": result.sections, "token_count": result.token_count }))
        })
    }

    #[tool(
        description = "Search a file for a keyword or regex pattern with surrounding context lines."
    )]
    async fn search_file(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<SearchFileParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "search_file", {
            let result = search_file(&root, params).map_err(err)?;
            ok_json(
                serde_json::json!({ "matches": result.matches, "token_count": result.token_count }),
            )
        })
    }

    #[tool(
        description = "Get key structure of a JSON or YAML file. Call before read_json_yaml_value to identify key paths."
    )]
    async fn read_json_yaml_keys(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadJsonYamlKeysParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_json_yaml_keys", {
            let key = delta_key("read_json_yaml_keys", &params);
            let result = read_json_yaml_keys(&root, params).map_err(err)?;
            self.ok_delta(
                key,
                serde_json::json!({ "keys": result.keys, "token_count": result.token_count }),
            )
        })
    }

    #[tool(
        description = "Get a specific value from a JSON or YAML file by dot-notation key path (e.g. 'dependencies.tokio')."
    )]
    async fn read_json_yaml_value(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadJsonYamlValueParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_json_yaml_value", {
            let key = delta_key("read_json_yaml_value", &params);
            let result = read_json_yaml_value(&root, params).map_err(err)?;
            self.ok_delta(
                key,
                serde_json::json!({ "value": result.value, "token_count": result.token_count }),
            )
        })
    }

    #[tool(
        description = "Get code skeleton (functions, structs, classes) with signatures only. Call before read_code_body."
    )]
    async fn read_code_skeleton(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadCodeSkeletonParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_code_skeleton", {
            let key = delta_key("read_code_skeleton", &params);
            let result = read_code_skeleton(&root, params).map_err(err)?;
            self.ok_delta(key, serde_json::json!({ "language": result.language, "skeleton": result.skeleton, "token_count": result.token_count }))
        })
    }

    #[tool(
        description = "Get full body of specific code items by ID from read_code_skeleton. Optional zoom controls detail: 'body' (default), 'sketch' (control-flow only), 'skeleton' (signatures only), or 'auto' (pick by the latest check_budget strategy). The chosen level is echoed back as zoom_applied."
    )]
    async fn read_code_body(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadCodeBodyParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_code_body", {
            let level = self.resolve_zoom(params.zoom.as_deref());
            let path = params.path.clone();
            let ids = params.ids.clone();

            match level {
                "skeleton" => {
                    let key = delta_key("read_code_body:skeleton", &path);
                    let result = read_code_skeleton(
                        &root,
                        ReadCodeSkeletonParams {
                            path: path.clone(),
                            include_blocks: None,
                        },
                    )
                    .map_err(err)?;
                    self.ok_delta(
                        key,
                        serde_json::json!({
                            "zoom_applied": "skeleton",
                            "language": result.language,
                            "skeleton": result.skeleton,
                            "token_count": result.token_count,
                        }),
                    )
                }
                "sketch" => {
                    let sk_params = ReadCodeSketchParams {
                        path: path.clone(),
                        ids,
                    };
                    let key = delta_key("read_code_body:sketch", &sk_params);
                    let result = read_code_sketch(&root, sk_params).map_err(err)?;
                    self.ok_delta(
                        key,
                        serde_json::json!({
                            "zoom_applied": "sketch",
                            "items": result.items,
                            "token_count": result.token_count,
                        }),
                    )
                }
                _ => {
                    let key = delta_key("read_code_body", &params);
                    let mut result = read_code_body(&root, params).map_err(err)?;
                    // Cross-tool dedup: stub bodies already sent this session (e.g. by read_context_pack).
                    for item in &mut result.items {
                        if item.content.starts_with("Error:") {
                            continue;
                        }
                        if let Some(stub) = self.dedup_body(&root, &path, &item.id, &item.content) {
                            item.content = stub;
                        }
                    }
                    let token_count = tools::fs::estimate_tokens(
                        &serde_json::to_string(&result.items).unwrap_or_default(),
                    );
                    self.ok_delta(
                        key,
                        serde_json::json!({
                            "zoom_applied": "body",
                            "items": result.items,
                            "token_count": token_count,
                        }),
                    )
                }
            }
        })
    }

    #[tool(
        description = "Zoom level between read_code_skeleton (signatures) and read_code_body (full source). Given skeleton IDs, returns each symbol's control-flow sketch: signature + branches/loops + block delimiters + call lines kept verbatim, runs of pure-data lines (assignments, literals) collapsed into '… N lines …'. Typically 60-70% smaller than the body — use it to understand what a function does before deciding whether you need the full body."
    )]
    async fn read_code_sketch(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadCodeSketchParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_code_sketch", {
            let key = delta_key("read_code_sketch", &params);
            let result = read_code_sketch(&root, params).map_err(err)?;
            self.ok_delta(
                key,
                serde_json::json!({ "items": result.items, "token_count": result.token_count }),
            )
        })
    }

    #[tool(
        description = "Get import/dependency graph for a code file. Returns what it imports and what files import it (imported_by). direction: \"imports\" | \"imported_by\" | \"both\"."
    )]
    async fn read_code_deps(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadCodeDepsParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_code_deps", {
            let result = read_code_deps(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path, "language": result.language,
                "imports": result.imports, "imported_by": result.imported_by,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Get a unified outline of any file. Auto-detects type: code → skeleton, markdown → TOC, json/yaml → keys. Single entry point — no need to know the file type first."
    )]
    async fn read_file_outline(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadFileOutlineParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_file_outline", {
            let key = delta_key("read_file_outline", &params);
            let result = read_file_outline(&root, params).map_err(err)?;
            self.ok_delta(
                key,
                serde_json::json!({
                    "path": result.path, "kind": result.kind, "language": result.language,
                    "outline": result.outline, "token_count": result.token_count,
                }),
            )
        })
    }

    #[tool(
        description = "One-call task context collection: ranks workspace files and symbols by relevance to a task description, returns ranked files + relevant signatures + top symbol bodies, greedily filled up to a token budget. Replaces the tree→search→skeleton→body round-trip sequence when starting a task. No subprocess needed (lexical ranking)."
    )]
    async fn read_context_pack(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadContextPackParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_context_pack", {
            let mut result = read_context_pack(&root, params).map_err(err)?;
            // Record each body in the cross-tool ledger so a later read_code_body for
            // the same symbol is stubbed; stub here too if it was already sent.
            for body in &mut result.bodies {
                if let Some(stub) = self.dedup_body(&root, &body.path, &body.id, &body.content) {
                    body.content = stub;
                }
            }
            ok_json(serde_json::json!({
                "keywords": result.keywords,
                "files": result.files,
                "symbols": result.symbols,
                "bodies": result.bodies,
                "bodies_omitted_for_budget": result.bodies_omitted_for_budget,
                "budget": result.budget,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Search code semantically using a natural language query. EXPENSIVE AND NOT A GREP SUBSTITUTE: this spawns a separate `claude -p` CLI process, which is a billed model call of its own, adds seconds of latency, and gives non-deterministic results. Requires the `claude` CLI installed and authenticated. Prefer search_file (regex) or read_code_skeleton + read_code_body when you can name what you are looking for; reach for this only when the query is genuinely conceptual."
    )]
    async fn semantic_search(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<SemanticSearchParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "semantic_search", {
            let result = semantic_search(&root, params).map_err(err)?;
            ok_json(serde_json::json!({ "items": result.items, "token_count": result.token_count }))
        })
    }

    #[tool(
        description = "Find all usages of a symbol name (function, struct, class, variable) across the workspace. Returns file path, line number, and context for each match. Max 100 results."
    )]
    async fn read_symbol_usages(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadSymbolUsagesParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_symbol_usages", {
            let result = read_symbol_usages(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "symbol": result.symbol, "usages": result.usages,
                "total": result.total, "truncated": result.truncated,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Get type definitions (interface/type/enum/struct) with field names from TypeScript, Go, or Rust files. More detailed than read_code_skeleton for type-heavy files."
    )]
    async fn read_type_skeleton(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadTypeSkeletonParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_type_skeleton", {
            let key = delta_key("read_type_skeleton", &params);
            let result = read_type_skeleton(&root, params).map_err(err)?;
            self.ok_delta(
                key,
                serde_json::json!({
                    "path": result.path, "language": result.language,
                    "types": result.types, "token_count": result.token_count,
                }),
            )
        })
    }

    #[tool(
        description = "Get the call graph for a function: what functions it calls, and which functions in the same file call it. Uses function_id from read_code_skeleton."
    )]
    async fn read_call_graph(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadCallGraphParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_call_graph", {
            let result = read_call_graph(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "function": result.function, "file": result.file,
                "calls": result.calls, "called_by_in_file": result.called_by_in_file,
                "cross_file_callees": result.cross_file_callees,
                "cross_file_callers": result.cross_file_callers,
                "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "List all workspace files sorted by estimated token count (largest first). Use to identify token-heavy files before reading. Supports glob filtering."
    )]
    async fn read_token_map(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadTokenMapParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_token_map", {
            let result = read_token_map(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "files": result.files, "total_tokens": result.total_tokens,
                "file_count": result.file_count, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Get codebase-wide statistics: total files/lines/tokens, per-language breakdown with %, and top-10 largest files. Much faster overview than read_token_map."
    )]
    async fn read_workspace_stats(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadWorkspaceStatsParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_workspace_stats", {
            let result = read_workspace_stats(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "total_files": result.total_files, "total_lines": result.total_lines,
                "total_tokens": result.total_tokens, "by_language": result.by_language,
                "largest_files": result.largest_files, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Find all types that implement a given interface/trait/abstract class across the workspace. Supports TypeScript, Rust, Java, Kotlin, Go, PHP, C#."
    )]
    async fn read_interface_conformance(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadInterfaceConformanceParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_interface_conformance", {
            let result = read_interface_conformance(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "name": result.name, "kind": result.kind,
                "implementations": result.implementations,
                "total": result.total, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Execute multiple read operations in one call (code_skeleton | code_body | markdown_section | json_value | file_outline). Reduces round-trips when you need several files at once. Pass factor:true to collapse near-identical results (migrations, fixtures) into one template + per-file unified diffs."
    )]
    async fn batch_read(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<BatchReadParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "batch_read", {
            let result = batch_read(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "results": result.results,
                "factored": result.factored,
                "total_token_count": result.total_token_count,
            }))
        })
    }
}
