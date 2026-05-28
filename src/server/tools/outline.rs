use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::security::safe_path;
use super::code::{ReadCodeSkeletonParams, read_code_skeleton};
use super::fs::estimate_tokens;
use super::json_yaml::{ReadJsonYamlKeysParams, read_json_yaml_keys};
use super::markdown::{ReadMarkdownTocParams, read_markdown_toc};
use super::notebook::{ReadNotebookCellsParams, read_notebook_cells};
use super::proto::{ReadProtoSchemaParams, read_proto_schema};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadFileOutlineParams {
    #[schemars(description = "Root-relative path to any file (code, markdown, json, yaml)")]
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReadFileOutlineResult {
    pub path: String,
    /// "code" | "markdown" | "json" | "yaml" | "unknown"
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub outline: serde_json::Value,
    pub token_count: usize,
}

const CODE_EXTS: &[&str] = &[
    "rs", "py", "js", "jsx", "ts", "tsx", "go",
    "c", "cpp", "h", "hpp", "java", "cs", "rb", "php", "swift", "kt", "scala",
];

pub fn read_file_outline(root: &Path, params: ReadFileOutlineParams) -> anyhow::Result<ReadFileOutlineResult> {
    let abs_path = safe_path(root, &params.path)?;
    let ext = abs_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        e if CODE_EXTS.contains(&e) => {
            let result = read_code_skeleton(root, ReadCodeSkeletonParams {
                path: params.path.clone(),
                include_blocks: None,
            })?;
            let token_count = result.token_count;
            let language = result.language.clone();
            let outline = serde_json::to_value(result.skeleton)?;
            Ok(ReadFileOutlineResult {
                path: params.path,
                kind: "code".to_string(),
                language: Some(language),
                outline,
                token_count,
            })
        }
        "md" | "markdown" | "mdx" => {
            let result = read_markdown_toc(root, ReadMarkdownTocParams { path: params.path.clone() })?;
            let token_count = result.token_count;
            let outline = serde_json::to_value(result.toc)?;
            Ok(ReadFileOutlineResult {
                path: params.path,
                kind: "markdown".to_string(),
                language: None,
                outline,
                token_count,
            })
        }
        "json" | "jsonc" => {
            let result = read_json_yaml_keys(root, ReadJsonYamlKeysParams {
                path: params.path.clone(),
                depth: None,
            })?;
            let token_count = result.token_count;
            let outline = serde_json::to_value(result.keys)?;
            Ok(ReadFileOutlineResult {
                path: params.path,
                kind: "json".to_string(),
                language: None,
                outline,
                token_count,
            })
        }
        "yaml" | "yml" => {
            let result = read_json_yaml_keys(root, ReadJsonYamlKeysParams {
                path: params.path.clone(),
                depth: None,
            })?;
            let token_count = result.token_count;
            let outline = serde_json::to_value(result.keys)?;
            Ok(ReadFileOutlineResult {
                path: params.path,
                kind: "yaml".to_string(),
                language: None,
                outline,
                token_count,
            })
        }
        "toml" => {
            let result = read_json_yaml_keys(root, ReadJsonYamlKeysParams {
                path: params.path.clone(),
                depth: None,
            })?;
            let token_count = result.token_count;
            let outline = serde_json::to_value(result.keys)?;
            Ok(ReadFileOutlineResult {
                path: params.path,
                kind: "toml".to_string(),
                language: None,
                outline,
                token_count,
            })
        }
        "ipynb" => {
            let result = read_notebook_cells(root, ReadNotebookCellsParams { path: params.path.clone() })?;
            let token_count = result.token_count;
            let outline = serde_json::to_value(result.cells)?;
            Ok(ReadFileOutlineResult {
                path: params.path,
                kind: "notebook".to_string(),
                language: None,
                outline,
                token_count,
            })
        }
        "proto" => {
            let result = read_proto_schema(root, ReadProtoSchemaParams { path: params.path.clone() })?;
            let token_count = result.token_count;
            let outline = serde_json::to_value(result.types)?;
            Ok(ReadFileOutlineResult {
                path: params.path,
                kind: "proto".to_string(),
                language: None,
                outline,
                token_count,
            })
        }
        _ => {
            // Unknown: return file size info only
            let size = abs_path.metadata().map(|m| m.len()).unwrap_or(0);
            let token_count = estimate_tokens(&size.to_string());
            Ok(ReadFileOutlineResult {
                path: params.path,
                kind: "unknown".to_string(),
                language: None,
                outline: serde_json::Value::Null,
                token_count,
            })
        }
    }
}
