use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use toml;

use crate::security::safe_path;
use super::fs::estimate_tokens;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadJsonYamlKeysParams {
    #[schemars(description = "Root-relative path to the JSON, YAML, or TOML file")]
    pub path: String,
    #[schemars(description = "Maximum key depth (default: 3)")]
    pub depth: Option<usize>,
}

pub struct ReadJsonYamlKeysResult {
    pub keys: Vec<String>,
    pub token_count: usize,
}

pub fn read_json_yaml_keys(root: &Path, params: ReadJsonYamlKeysParams) -> anyhow::Result<ReadJsonYamlKeysResult> {
    let path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&path)?;
    let value = parse_file(&path, &content)?;
    let depth = params.depth.unwrap_or(3);
    let mut keys = Vec::new();
    collect_keys(&value, "", depth, 0, &mut keys);
    let json = serde_json::to_string(&keys).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadJsonYamlKeysResult { keys, token_count })
}

fn collect_keys(value: &Value, prefix: &str, max_depth: usize, current_depth: usize, keys: &mut Vec<String>) {
    if current_depth >= max_depth {
        return;
    }
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", prefix, k)
                };
                keys.push(key.clone());
                collect_keys(v, &key, max_depth, current_depth + 1, keys);
            }
        }
        Value::Array(arr)
            if !arr.is_empty() => {
                let key = format!("{}[0]", prefix);
                collect_keys(&arr[0], &key, max_depth, current_depth + 1, keys);
            }
        _ => {}
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadJsonYamlValueParams {
    #[schemars(description = "Root-relative path to the JSON, YAML, or TOML file")]
    pub path: String,
    #[schemars(description = "Dot-notation key path, e.g. 'dependencies.tokio' or 'items[0].name'")]
    pub key_path: String,
}

pub struct ReadJsonYamlValueResult {
    pub value: Value,
    pub token_count: usize,
}

pub fn read_json_yaml_value(root: &Path, params: ReadJsonYamlValueParams) -> anyhow::Result<ReadJsonYamlValueResult> {
    let path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&path)?;
    let value = parse_file(&path, &content)?;
    let result = resolve_path(&value, &params.key_path)?;
    let json = serde_json::to_string(&result).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadJsonYamlValueResult { value: result.clone(), token_count })
}

pub(crate) fn parse_file(path: &Path, content: &str) -> anyhow::Result<Value> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext {
        "yaml" | "yml" => {
            let v: Value = serde_yaml::from_str(content)?;
            Ok(v)
        }
        "toml" => {
            let toml_val: toml::Value = toml::from_str(content)?;
            let json_str = serde_json::to_string(&toml_val)?;
            let v: Value = serde_json::from_str(&json_str)?;
            Ok(v)
        }
        _ => {
            let v: Value = serde_json::from_str(content)?;
            Ok(v)
        }
    }
}

fn resolve_path<'a>(value: &'a Value, path: &str) -> anyhow::Result<&'a Value> {
    if path.is_empty() {
        return Ok(value);
    }
    let mut current = value;
    for segment in tokenize_path(path) {
        match segment {
            PathSegment::Key(k) => {
                current = current
                    .as_object()
                    .and_then(|o| o.get(&k))
                    .ok_or_else(|| anyhow::anyhow!("key '{}' not found", k))?;
            }
            PathSegment::Index(i) => {
                current = current
                    .as_array()
                    .and_then(|a| a.get(i))
                    .ok_or_else(|| anyhow::anyhow!("index {} out of bounds", i))?;
            }
        }
    }
    Ok(current)
}

pub(crate) enum PathSegment {
    Key(String),
    Index(usize),
}

pub(crate) fn tokenize_path(path: &str) -> Vec<PathSegment> {
    let mut segments = Vec::new();
    let mut current = String::new();

    for ch in path.chars() {
        match ch {
            '.' => {
                if !current.is_empty() {
                    segments.push(PathSegment::Key(current.clone()));
                    current.clear();
                }
            }
            '[' => {
                if !current.is_empty() {
                    segments.push(PathSegment::Key(current.clone()));
                    current.clear();
                }
            }
            ']' => {
                if let Ok(i) = current.parse::<usize>() {
                    segments.push(PathSegment::Index(i));
                    current.clear();
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        segments.push(PathSegment::Key(current));
    }
    segments
}
