use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

use crate::security::safe_path;
use super::fs::estimate_tokens;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadOpenApiParams {
    #[schemars(description = "Root-relative path to an OpenAPI / Swagger JSON or YAML file")]
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenApiEndpoint {
    pub method: String,
    pub path: String,
    pub operation_id: Option<String>,
    pub summary: Option<String>,
    pub tags: Vec<String>,
    pub parameters: Vec<String>,
    pub request_body: Option<String>,
    pub responses: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReadOpenApiResult {
    pub title: Option<String>,
    pub version: Option<String>,
    pub base_url: Option<String>,
    pub spec_version: String,
    pub endpoints: Vec<OpenApiEndpoint>,
    pub token_count: usize,
}

pub fn read_openapi(root: &Path, params: ReadOpenApiParams) -> anyhow::Result<ReadOpenApiResult> {
    let path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", params.path, e))?;

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let doc: Value = if ext == "json" {
        serde_json::from_str(&content)
            .map_err(|e| anyhow::anyhow!("JSON parse error: {e}"))?
    } else {
        serde_yaml::from_str(&content)
            .map_err(|e| anyhow::anyhow!("YAML parse error: {e}"))?
    };

    let spec_version = if let Some(v) = doc.get("openapi").and_then(|v| v.as_str()) {
        v.to_string()
    } else if let Some(v) = doc.get("swagger").and_then(|v| v.as_str()) {
        format!("swagger/{v}")
    } else {
        anyhow::bail!("'openapi' または 'swagger' キーが見つかりません。OpenAPI/Swagger ファイルか確認してください");
    };

    let title = doc.pointer("/info/title").and_then(|v| v.as_str()).map(String::from);
    let version = doc.pointer("/info/version")
        .map(|v| match v { Value::String(s) => s.clone(), other => other.to_string() });

    let base_url = doc.pointer("/servers/0/url")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| doc.get("basePath").and_then(|v| v.as_str()).map(String::from));

    let mut endpoints: Vec<OpenApiEndpoint> = Vec::new();
    const HTTP_METHODS: &[&str] = &["get", "post", "put", "delete", "patch", "options", "head"];

    if let Some(paths) = doc.get("paths").and_then(|v| v.as_object()) {
        for (path_key, path_val) in paths {
            for method in HTTP_METHODS {
                let Some(op) = path_val.get(method) else { continue };

                let operation_id = op.get("operationId").and_then(|v| v.as_str()).map(String::from);
                let summary = op.get("summary").and_then(|v| v.as_str()).map(String::from);

                let tags: Vec<String> = op.get("tags")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|t| t.as_str()).map(String::from).collect())
                    .unwrap_or_default();

                let parameters: Vec<String> = op.get("parameters")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|p| {
                        let name = p.get("name")?.as_str()?;
                        let loc = p.get("in").and_then(|v| v.as_str()).unwrap_or("?");
                        let req = p.get("required").and_then(|v| v.as_bool()).unwrap_or(false);
                        Some(if req { format!("{name} ({loc}, required)") } else { format!("{name} ({loc})") })
                    }).collect())
                    .unwrap_or_default();

                let request_body = op.get("requestBody")
                    .and_then(|rb| rb.get("content"))
                    .and_then(|c| c.as_object())
                    .map(|obj| obj.keys().cloned().collect::<Vec<_>>().join(", "));

                let responses: Vec<String> = op.get("responses")
                    .and_then(|v| v.as_object())
                    .map(|obj| obj.iter().map(|(code, resp)| {
                        let desc = resp.get("description").and_then(|v| v.as_str()).unwrap_or("");
                        if desc.is_empty() { code.clone() } else { format!("{code} {desc}") }
                    }).collect())
                    .unwrap_or_default();

                endpoints.push(OpenApiEndpoint {
                    method: method.to_uppercase(),
                    path: path_key.clone(),
                    operation_id,
                    summary,
                    tags,
                    parameters,
                    request_body,
                    responses,
                });
            }
        }
    }

    let json = serde_json::to_string(&endpoints).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadOpenApiResult { title, version, base_url, spec_version, endpoints, token_count })
}
