use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::code::{ReadCodeBodyParams, ReadCodeSkeletonParams, read_code_body, read_code_skeleton};
use super::fs::estimate_tokens;
use super::json_yaml::{ReadJsonYamlValueParams, read_json_yaml_value};
use super::markdown::{ReadMarkdownSectionParams, read_markdown_section};
use super::outline::{ReadFileOutlineParams, read_file_outline};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BatchReadItem {
    #[schemars(description = "Client-assigned ID to correlate results")]
    pub id: String,
    #[schemars(description = "Operation: code_skeleton | code_body | markdown_section | json_value | file_outline")]
    pub operation: String,
    #[schemars(description = "Root-relative file path")]
    pub path: String,
    #[schemars(description = "Skeleton IDs for code_body operation")]
    pub ids: Option<Vec<String>>,
    #[schemars(description = "Heading anchors for markdown_section operation")]
    pub anchors: Option<Vec<String>>,
    #[schemars(description = "Key path for json_value operation (e.g. 'dependencies.tokio')")]
    pub key_path: Option<String>,
    #[schemars(description = "Include block constructs for code_skeleton operation")]
    pub include_blocks: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BatchReadParams {
    #[schemars(description = "List of read operations to execute")]
    pub reads: Vec<BatchReadItem>,
}

#[derive(Debug, Serialize)]
pub struct BatchReadItemResult {
    pub id: String,
    pub ok: bool,
    pub data: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub token_count: usize,
}

#[derive(Debug, Serialize)]
pub struct BatchReadResult {
    pub results: Vec<BatchReadItemResult>,
    pub total_token_count: usize,
}

pub fn batch_read(root: &Path, params: BatchReadParams) -> anyhow::Result<BatchReadResult> {
    let mut results = Vec::new();

    for item in params.reads {
        let (ok, data, error, token_count) = match execute_item(root, &item) {
            Ok((data, tc)) => (true, data, None, tc),
            Err(e) => (false, Value::Null, Some(e.to_string()), 0),
        };
        results.push(BatchReadItemResult { id: item.id, ok, data, error, token_count });
    }

    let total_token_count = results.iter().map(|r| r.token_count).sum();
    Ok(BatchReadResult { results, total_token_count })
}

fn execute_item(root: &Path, item: &BatchReadItem) -> anyhow::Result<(Value, usize)> {
    match item.operation.as_str() {
        "code_skeleton" => {
            let result = read_code_skeleton(root, ReadCodeSkeletonParams {
                path: item.path.clone(),
                include_blocks: item.include_blocks,
            })?;
            let data = serde_json::json!({
                "language": result.language,
                "skeleton": result.skeleton,
            });
            Ok((data, result.token_count))
        }
        "code_body" => {
            let ids = item.ids.clone().unwrap_or_default();
            anyhow::ensure!(!ids.is_empty(), "code_body requires ids");
            let result = read_code_body(root, ReadCodeBodyParams {
                path: item.path.clone(),
                ids,
            })?;
            let data = serde_json::to_value(&result.items)?;
            Ok((data, result.token_count))
        }
        "markdown_section" => {
            let anchors = item.anchors.clone().unwrap_or_default();
            anyhow::ensure!(!anchors.is_empty(), "markdown_section requires anchors");
            let result = read_markdown_section(root, ReadMarkdownSectionParams {
                path: item.path.clone(),
                anchors,
            })?;
            let data = serde_json::to_value(&result.sections)?;
            Ok((data, result.token_count))
        }
        "json_value" => {
            let key_path = item.key_path.clone().unwrap_or_default();
            anyhow::ensure!(!key_path.is_empty(), "json_value requires key_path");
            let result = read_json_yaml_value(root, ReadJsonYamlValueParams {
                path: item.path.clone(),
                key_path,
            })?;
            let tc = estimate_tokens(&result.value.to_string());
            Ok((result.value, tc))
        }
        "file_outline" => {
            let result = read_file_outline(root, ReadFileOutlineParams {
                path: item.path.clone(),
            })?;
            let data = serde_json::to_value(&result)?;
            let tc = estimate_tokens(&data.to_string());
            Ok((data, tc))
        }
        other => anyhow::bail!("Unknown operation: {}", other),
    }
}
