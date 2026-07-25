//! Schema and DSL tool handlers — the `schema` category of `help()`.
//!
//! Registered as `schema_router` and merged into the server's router in
//! [`super::tool_router`].

use crate::server::*;

#[tool_router(router = schema_router, vis = "pub(crate)")]
impl T0k3nServer {
    #[tool(
        description = "Parse an OpenAPI / Swagger spec (JSON or YAML) and return a compact endpoint summary: method, path, operation_id, summary, parameters, request body, and responses."
    )]
    async fn read_openapi(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadOpenApiParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_openapi", {
            let result = read_openapi(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "title": result.title, "version": result.version,
                "base_url": result.base_url, "spec_version": result.spec_version,
                "endpoints": result.endpoints, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Extract environment variable definitions from .env.example / .env.sample / .env.template / docker-compose.yml. Returns key, description (from comments), default value, and required flag. Omit path to auto-scan workspace root."
    )]
    async fn read_env_schema(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadEnvSchemaParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_env_schema", {
            let result = read_env_schema(&root, params).map_err(err)?;
            ok_json(
                serde_json::json!({ "vars": result.vars, "sources": result.sources, "token_count": result.token_count }),
            )
        })
    }

    #[tool(
        description = "Read a Prisma or SQL schema. Omit table to list every table/model with its kind and field count; pass a table name to get its full field definitions. Omit path to auto-detect the schema file."
    )]
    async fn read_db(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadDbParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_db", {
            match params.table {
                Some(table) => {
                    // The table list carries the path that found it, so by the time a
                    // caller names a table they have a concrete path to pass back.
                    let path = params.path.ok_or_else(|| {
                        err(anyhow::anyhow!(
                            "'path' is required when 'table' is given — call read_db without \
                             'table' first to locate the schema file"
                        ))
                    })?;
                    let result =
                        read_db_table(&root, ReadDbTableParams { path, table }).map_err(err)?;
                    ok_json(serde_json::json!({
                        "name": result.name, "kind": result.kind,
                        "fields": result.fields, "token_count": result.token_count,
                    }))
                }
                None => {
                    let result = read_db_schema(&root, ReadDbSchemaParams { path: params.path })
                        .map_err(err)?;
                    ok_json(serde_json::json!({
                        "path": result.path, "format": result.format,
                        "tables": result.tables, "token_count": result.token_count,
                    }))
                }
            }
        })
    }

    #[tool(
        description = "Read a CSS/SCSS/Less file. Omit ids to list every selector with its property count and an id; pass those ids back to get the full rule bodies."
    )]
    async fn read_css(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadCssParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_css", {
            match params.ids {
                Some(ids) => {
                    let result = read_css_body(
                        &root,
                        ReadCssBodyParams {
                            path: params.path,
                            ids,
                        },
                    )
                    .map_err(err)?;
                    ok_json(
                        serde_json::json!({ "items": result.items, "token_count": result.token_count }),
                    )
                }
                None => {
                    let result =
                        read_css_skeleton(&root, ReadCssSkeletonParams { path: params.path })
                            .map_err(err)?;
                    ok_json(serde_json::json!({
                        "path": result.path, "selectors": result.selectors, "token_count": result.token_count,
                    }))
                }
            }
        })
    }

    #[tool(
        description = "Read a GraphQL schema file. Omit type_name to list every type/input/enum/interface; pass a type name to get its full field definitions."
    )]
    async fn read_graphql(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadGraphqlParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_graphql", {
            match params.type_name {
                Some(type_name) => {
                    let result = read_graphql_type(
                        &root,
                        ReadGraphqlTypeParams {
                            path: params.path,
                            type_name,
                        },
                    )
                    .map_err(err)?;
                    ok_json(serde_json::json!({
                        "name": result.name, "kind": result.kind,
                        "fields": result.fields, "token_count": result.token_count,
                    }))
                }
                None => {
                    let result =
                        read_graphql_schema(&root, ReadGraphqlSchemaParams { path: params.path })
                            .map_err(err)?;
                    ok_json(serde_json::json!({
                        "path": result.path, "types": result.types, "token_count": result.token_count,
                    }))
                }
            }
        })
    }

    #[tool(
        description = "Read a .proto (Protocol Buffers) file. Omit type_name to list every message/service/enum; pass a name to get its full field definitions."
    )]
    async fn read_proto(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadProtoParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_proto", {
            match params.type_name {
                Some(type_name) => {
                    let result = read_proto_type(
                        &root,
                        ReadProtoTypeParams {
                            path: params.path,
                            type_name,
                        },
                    )
                    .map_err(err)?;
                    ok_json(serde_json::json!({
                        "name": result.name, "kind": result.kind,
                        "fields": result.fields, "token_count": result.token_count,
                    }))
                }
                None => {
                    let result =
                        read_proto_schema(&root, ReadProtoSchemaParams { path: params.path })
                            .map_err(err)?;
                    ok_json(serde_json::json!({
                        "path": result.path, "syntax": result.syntax, "package": result.package,
                        "types": result.types, "token_count": result.token_count,
                    }))
                }
            }
        })
    }

    #[tool(
        description = "Parse package.json / Cargo.toml / go.mod / pyproject.toml / pom.xml / build.gradle into a unified dependency list. Faster than read_json_yaml_value for multi-ecosystem projects."
    )]
    async fn read_package_manifest(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadPackageManifestParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_package_manifest", {
            let result = read_package_manifest(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "manifests": result.manifests, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Parse CI pipeline configs (GitHub Actions / GitLab CI / CircleCI) into structured workflow/job/step summary. Omit path to auto-scan workspace."
    )]
    async fn read_ci_pipeline(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadCiPipelineParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_ci_pipeline", {
            let result = read_ci_pipeline(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "pipelines": result.pipelines, "token_count": result.token_count,
            }))
        })
    }
}
