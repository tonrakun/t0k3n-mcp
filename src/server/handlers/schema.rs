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
        description = "Get table/model list from a Prisma or SQL schema file. Returns name, kind, and field count. Call read_db_table for field details of a specific table."
    )]
    async fn read_db_schema(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadDbSchemaParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_db_schema", {
            let result = read_db_schema(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path, "format": result.format,
                "tables": result.tables, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Get full field definitions for a specific table or model from a Prisma or SQL schema. Call read_db_schema first to get the table list."
    )]
    async fn read_db_table(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadDbTableParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_db_table", {
            let result = read_db_table(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "name": result.name, "kind": result.kind,
                "fields": result.fields, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Get CSS/SCSS/Less selector list with property counts. Returns IDs for use with read_css_body."
    )]
    async fn read_css_skeleton(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadCssSkeletonParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_css_skeleton", {
            let result = read_css_skeleton(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path, "selectors": result.selectors, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Get full CSS rule content for specific selectors by ID. Call read_css_skeleton first to get selector IDs."
    )]
    async fn read_css_body(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadCssBodyParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_css_body", {
            let result = read_css_body(&root, params).map_err(err)?;
            ok_json(serde_json::json!({ "items": result.items, "token_count": result.token_count }))
        })
    }

    #[tool(
        description = "Get type/input/enum/interface list from a GraphQL schema file. Returns IDs for use with read_graphql_type."
    )]
    async fn read_graphql_schema(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadGraphqlSchemaParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_graphql_schema", {
            let result = read_graphql_schema(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path, "types": result.types, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Get full field definitions for a specific GraphQL type. Call read_graphql_schema first to get the type list."
    )]
    async fn read_graphql_type(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadGraphqlTypeParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_graphql_type", {
            let result = read_graphql_type(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "name": result.name, "kind": result.kind,
                "fields": result.fields, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Get message/service/enum list from a .proto (Protocol Buffers) file. Returns IDs for use with read_proto_type."
    )]
    async fn read_proto_schema(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadProtoSchemaParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_proto_schema", {
            let result = read_proto_schema(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "path": result.path, "syntax": result.syntax, "package": result.package,
                "types": result.types, "token_count": result.token_count,
            }))
        })
    }

    #[tool(
        description = "Get full field definitions for a specific message, service, or enum in a .proto file. Call read_proto_schema first to get the type list."
    )]
    async fn read_proto_type(
        &self,
        EffectiveRoot(root): EffectiveRoot,
        Parameters(params): Parameters<ReadProtoTypeParams>,
    ) -> Result<CallToolResult, McpError> {
        instrument!(self, "read_proto_type", {
            let result = read_proto_type(&root, params).map_err(err)?;
            ok_json(serde_json::json!({
                "name": result.name, "kind": result.kind,
                "fields": result.fields, "token_count": result.token_count,
            }))
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
