use std::path::Path;

use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::fs::estimate_tokens;
use crate::security::{rel_display, safe_path};

// ─── read_proto_schema ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadProtoSchemaParams {
    #[schemars(description = "Root-relative path to a .proto file.")]
    pub path: String,
}

#[derive(Debug, Serialize, JsonSchema, Clone)]
pub struct ProtoTypeEntry {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub field_count: usize,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Serialize)]
pub struct ReadProtoSchemaResult {
    pub path: String,
    pub syntax: Option<String>,
    pub package: Option<String>,
    pub types: Vec<ProtoTypeEntry>,
    pub token_count: usize,
}

pub fn read_proto_schema(
    root: &Path,
    params: ReadProtoSchemaParams,
) -> anyhow::Result<ReadProtoSchemaResult> {
    let file_path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| anyhow::anyhow!("ファイル読み取り失敗: {e}"))?;

    let rel = rel_display(root, &file_path);

    let syntax = extract_proto_meta(&content, r#"syntax\s*=\s*"([^"]+)""#);
    let package = extract_proto_meta(&content, r"^package\s+([\w.]+)\s*;");
    let types = parse_proto_schema(&content);

    let json = serde_json::to_string(&types).unwrap_or_default();
    let token_count = estimate_tokens(&json);

    Ok(ReadProtoSchemaResult {
        path: rel,
        syntax,
        package,
        types,
        token_count,
    })
}

fn extract_proto_meta(content: &str, pattern: &str) -> Option<String> {
    let re = Regex::new(pattern).unwrap();
    for line in content.lines() {
        if let Some(cap) = re.captures(line.trim()) {
            return Some(cap[1].to_string());
        }
    }
    None
}

pub fn parse_proto_schema(content: &str) -> Vec<ProtoTypeEntry> {
    let lines: Vec<&str> = content.lines().collect();
    let mut items = Vec::new();
    let re = Regex::new(r"^(message|service|enum)\s+(\w+)").unwrap();

    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.is_empty() {
            i += 1;
            continue;
        }

        if let Some(cap) = re.captures(trimmed) {
            let kind = cap[1].to_string();
            let name = cap[2].to_string();
            let start_line = i + 1;

            let mut depth: i32 = 0;
            let mut field_count = 0;
            let mut j = i;

            while j < lines.len() {
                let l = lines[j].trim();
                let opens = l.chars().filter(|&c| c == '{').count() as i32;
                let closes = l.chars().filter(|&c| c == '}').count() as i32;
                depth += opens - closes;

                if depth == 1
                    && j > i
                    && !l.is_empty()
                    && !l.starts_with("//")
                    && (l.contains('=') || l.starts_with("rpc "))
                {
                    field_count += 1;
                }

                if depth <= 0 && (opens > 0 || closes > 0) {
                    break;
                }
                j += 1;
            }

            let end_line = (j + 1).min(lines.len());
            let id = format!("{}:{}-{}", kind, start_line, end_line);
            items.push(ProtoTypeEntry {
                id,
                name,
                kind,
                field_count,
                start_line,
                end_line,
            });
            i = j + 1;
            continue;
        }

        i += 1;
    }

    items
}

// ─── read_proto_type ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadProtoTypeParams {
    #[schemars(description = "Root-relative path to a .proto file.")]
    pub path: String,
    #[schemars(description = "Name of the message, service, or enum to get details for.")]
    pub type_name: String,
}

#[derive(Debug, Serialize)]
pub struct ProtoFieldEntry {
    pub name: String,
    pub field_type: String,
    pub number: Option<u32>,
    pub label: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReadProtoTypeResult {
    pub name: String,
    pub kind: String,
    pub fields: Vec<ProtoFieldEntry>,
    pub token_count: usize,
}

pub fn read_proto_type(
    root: &Path,
    params: ReadProtoTypeParams,
) -> anyhow::Result<ReadProtoTypeResult> {
    let file_path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| anyhow::anyhow!("ファイル読み取り失敗: {e}"))?;

    let types = parse_proto_schema(&content);
    let entry = types
        .iter()
        .find(|t| t.name == params.type_name)
        .ok_or_else(|| anyhow::anyhow!("'{}' が見つかりません", params.type_name))?
        .clone();

    let lines: Vec<&str> = content.lines().collect();
    let from = entry.start_line.saturating_sub(1);
    let to = entry.end_line.min(lines.len());
    let body_lines = &lines[from..to];

    let fields = match entry.kind.as_str() {
        "message" => parse_message_fields(body_lines),
        "service" => parse_service_rpcs(body_lines),
        "enum" => parse_enum_values(body_lines),
        _ => vec![],
    };

    let json = serde_json::to_string(&fields).unwrap_or_default();
    let token_count = estimate_tokens(&json);

    Ok(ReadProtoTypeResult {
        name: params.type_name,
        kind: entry.kind,
        fields,
        token_count,
    })
}

fn parse_message_fields(lines: &[&str]) -> Vec<ProtoFieldEntry> {
    let re =
        Regex::new(r"^\s*(optional|repeated|required)?\s*([\w.]+)\s+(\w+)\s*=\s*(\d+)").unwrap();

    lines
        .iter()
        .filter_map(|line| {
            let t = line.trim();
            if t.starts_with("//") || t.contains('{') || t.contains('}') {
                return None;
            }
            re.captures(t).map(|cap| ProtoFieldEntry {
                label: cap.get(1).map(|m| m.as_str().to_string()),
                field_type: cap[2].to_string(),
                name: cap[3].to_string(),
                number: cap[4].parse().ok(),
            })
        })
        .collect()
}

fn parse_service_rpcs(lines: &[&str]) -> Vec<ProtoFieldEntry> {
    let re =
        Regex::new(r"^\s*rpc\s+(\w+)\s*\(\s*([\w.]+)\s*\)\s*returns\s*\(\s*([\w.]+)\s*\)").unwrap();

    lines
        .iter()
        .filter_map(|line| {
            re.captures(line.trim()).map(|cap| ProtoFieldEntry {
                name: cap[1].to_string(),
                field_type: format!("({}) → ({})", &cap[2], &cap[3]),
                number: None,
                label: None,
            })
        })
        .collect()
}

fn parse_enum_values(lines: &[&str]) -> Vec<ProtoFieldEntry> {
    let re = Regex::new(r"^\s*(\w+)\s*=\s*(\d+)").unwrap();

    lines
        .iter()
        .filter_map(|line| {
            let t = line.trim();
            if t.starts_with("//") || t.contains('{') || t.contains('}') {
                return None;
            }
            re.captures(t).map(|cap| ProtoFieldEntry {
                name: cap[1].to_string(),
                field_type: "enum_value".to_string(),
                number: cap[2].parse().ok(),
                label: None,
            })
        })
        .collect()
}
