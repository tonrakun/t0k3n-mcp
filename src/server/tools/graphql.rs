use std::path::Path;

use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::fs::estimate_tokens;
use crate::security::{rel_display, safe_path};

// ─── read_graphql_schema ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadGraphqlSchemaParams {
    #[schemars(description = "Root-relative path to a .graphql or .gql file.")]
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct GraphqlTypeEntry {
    pub id: String,
    pub name: String,
    pub kind: String, // type | input | enum | interface | union | scalar | schema
    pub field_count: usize,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Serialize)]
pub struct ReadGraphqlSchemaResult {
    pub path: String,
    pub types: Vec<GraphqlTypeEntry>,
    pub token_count: usize,
}

pub fn read_graphql_schema(
    root: &Path,
    params: ReadGraphqlSchemaParams,
) -> anyhow::Result<ReadGraphqlSchemaResult> {
    let file_path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| anyhow::anyhow!("ファイル読み取り失敗: {e}"))?;
    let rel = rel_display(root, &file_path);

    let types = parse_graphql_schema(&content);
    let json = serde_json::to_string(&types).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadGraphqlSchemaResult {
        path: rel,
        types,
        token_count,
    })
}

pub fn parse_graphql_schema(content: &str) -> Vec<GraphqlTypeEntry> {
    let lines: Vec<&str> = content.lines().collect();
    let mut items = Vec::new();
    // Matches: (extend )? type|input|enum|interface|union|scalar|schema  Name  ...{
    let re = Regex::new(r"^(?:extend\s+)?(type|input|enum|interface|union|scalar|schema)\s+(\w+)")
        .unwrap();

    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            i += 1;
            continue;
        }

        if let Some(cap) = re.captures(trimmed) {
            let kind = cap[1].to_string();
            let name = cap[2].to_string();
            let start_line = i + 1; // 1-indexed

            // Walk forward to find matching braces
            let mut depth: i32 = 0;
            let mut j = i;
            let mut field_count = 0;

            while j < lines.len() {
                let l = lines[j].trim();
                let opens = l.chars().filter(|&c| c == '{').count() as i32;
                let closes = l.chars().filter(|&c| c == '}').count() as i32;
                depth += opens - closes;

                // Count non-empty, non-comment lines inside the body at depth 1
                if depth == 1
                    && j > i
                    && !l.is_empty()
                    && !l.starts_with('#')
                    && !l.contains('{')
                    && !l.contains('}')
                {
                    field_count += 1;
                }

                if depth <= 0 && (opens > 0 || closes > 0) {
                    break;
                }
                j += 1;
            }

            // scalar / union may have no braces — end_line == start_line
            let end_line = if depth <= 0 {
                (j + 1).min(lines.len())
            } else {
                start_line
            };
            let id = format!("type:{}-{}", start_line, end_line);
            items.push(GraphqlTypeEntry {
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

// ─── read_graphql_type ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadGraphqlTypeParams {
    #[schemars(
        description = "Root-relative path to the .graphql or .gql file (from read_graphql_schema)."
    )]
    pub path: String,
    #[schemars(
        description = "Type name to retrieve fields for (from read_graphql_schema types list)."
    )]
    pub type_name: String,
}

#[derive(Debug, Serialize)]
pub struct GraphqlFieldEntry {
    pub name: String,
    pub field_type: String,
    pub args: String,
    pub description: String,
}

#[derive(Debug, Serialize)]
pub struct ReadGraphqlTypeResult {
    pub name: String,
    pub kind: String,
    pub fields: Vec<GraphqlFieldEntry>,
    pub token_count: usize,
}

pub fn read_graphql_type(
    root: &Path,
    params: ReadGraphqlTypeParams,
) -> anyhow::Result<ReadGraphqlTypeResult> {
    let file_path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| anyhow::anyhow!("ファイル読み取り失敗: {e}"))?;

    let types = parse_graphql_schema(&content);
    let entry = types
        .iter()
        .find(|t| t.name == params.type_name)
        .ok_or_else(|| anyhow::anyhow!("型 '{}' が見つかりません", params.type_name))?;

    let lines: Vec<&str> = content.lines().collect();
    let from = entry.start_line.saturating_sub(1);
    let to = entry.end_line.min(lines.len());
    let body_lines = &lines[from..to];

    let re_field = Regex::new(r"^\s*(\w+)(\([^)]*\))?\s*:\s*(.+)$").unwrap();
    let mut fields = Vec::new();
    let mut pending_desc = String::new();

    for line in body_lines {
        let t = line.trim();
        if t.starts_with('#') {
            pending_desc = t.trim_start_matches('#').trim().to_string();
            continue;
        }
        if t.contains('{') || t.contains('}') || t.is_empty() {
            pending_desc = String::new();
            continue;
        }
        if let Some(cap) = re_field.captures(t) {
            let name = cap[1].to_string();
            let args = cap
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let field_type = cap[3]
                .split('#')
                .next()
                .unwrap_or("")
                .trim()
                .trim_end_matches(',')
                .to_string();
            fields.push(GraphqlFieldEntry {
                name,
                field_type,
                args,
                description: std::mem::take(&mut pending_desc),
            });
        } else {
            pending_desc = String::new();
        }
    }

    let json = serde_json::to_string(&fields).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadGraphqlTypeResult {
        name: params.type_name,
        kind: entry.kind.clone(),
        fields,
        token_count,
    })
}
