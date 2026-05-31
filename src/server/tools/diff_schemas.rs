use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::fs::estimate_tokens;
use crate::security::safe_path;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DiffSchemasParams {
    #[schemars(description = "Root-relative path to the schema file (OpenAPI .yaml/.json, Prisma .prisma, SQL .sql, TypeScript .ts/.d.ts)")]
    pub path: String,
    #[schemars(description = "Git ref for the 'before' state (default: HEAD~1). Examples: HEAD~1, main, abc1234")]
    pub before_ref: Option<String>,
    #[schemars(description = "Git ref for the 'after' state (default: HEAD = working tree)")]
    pub after_ref: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SchemaDiffEntry {
    pub name: String,
    pub kind: String,   // e.g. "endpoint", "table", "type", "field"
    pub change: String, // "added" | "removed" | "modified"
    pub detail: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DiffSchemasResult {
    pub path: String,
    pub schema_type: String, // "openapi" | "prisma" | "sql" | "typescript" | "unknown"
    pub before_ref: String,
    pub after_ref: String,
    pub added: Vec<SchemaDiffEntry>,
    pub removed: Vec<SchemaDiffEntry>,
    pub modified: Vec<SchemaDiffEntry>,
    pub total_changes: usize,
    pub token_count: usize,
}

pub fn diff_schemas(root: &Path, params: DiffSchemasParams) -> anyhow::Result<DiffSchemasResult> {
    let file_path = safe_path(root, &params.path).map_err(|e| anyhow::anyhow!("{e}"))?;
    let before_ref = params.before_ref.as_deref().unwrap_or("HEAD~1").to_string();
    let after_ref = params.after_ref.clone();

    let schema_type = detect_schema_type(&params.path);

    // Get "before" content from git
    let before_content = git_show(root, &before_ref, &params.path).unwrap_or_default();

    // Get "after" content
    let after_content = if let Some(ref aref) = after_ref {
        git_show(root, aref, &params.path).unwrap_or_default()
    } else {
        std::fs::read_to_string(&file_path).unwrap_or_default()
    };

    let after_label = after_ref.as_deref().unwrap_or("working tree").to_string();

    let (added, removed, modified) = match schema_type.as_str() {
        "openapi" => diff_openapi(&before_content, &after_content),
        "prisma" | "sql" => diff_db_schema(&before_content, &after_content, &schema_type),
        "typescript" => diff_typescript(&before_content, &after_content),
        _ => diff_generic_lines(&before_content, &after_content),
    };

    let total_changes = added.len() + removed.len() + modified.len();
    let json = serde_json::json!({
        "added": added, "removed": removed, "modified": modified
    });
    let token_count = estimate_tokens(&json.to_string());

    Ok(DiffSchemasResult {
        path: params.path,
        schema_type,
        before_ref,
        after_ref: after_label,
        added,
        removed,
        modified,
        total_changes,
        token_count,
    })
}

fn git_show(root: &Path, git_ref: &str, file_path: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["show", &format!("{}:{}", git_ref, file_path)])
        .current_dir(root)
        .output()
        .ok()?;
    if output.status.success() {
        String::from_utf8(output.stdout).ok()
    } else {
        None
    }
}

fn detect_schema_type(path: &str) -> String {
    let lower = path.to_lowercase();
    if lower.ends_with(".prisma") {
        return "prisma".to_string();
    }
    if lower.ends_with(".sql") {
        return "sql".to_string();
    }
    if lower.ends_with(".ts") || lower.ends_with(".d.ts") {
        return "typescript".to_string();
    }
    if lower.ends_with(".yaml") || lower.ends_with(".yml") || lower.ends_with(".json") {
        // Heuristic: if path contains openapi/swagger or we'll check content
        if lower.contains("openapi") || lower.contains("swagger") || lower.contains("api") {
            return "openapi".to_string();
        }
    }
    "unknown".to_string()
}

// ─── OpenAPI diff ────────────────────────────────────────────────────────────

fn diff_openapi(
    before: &str,
    after: &str,
) -> (Vec<SchemaDiffEntry>, Vec<SchemaDiffEntry>, Vec<SchemaDiffEntry>) {
    let before_endpoints = parse_openapi_endpoints(before);
    let after_endpoints = parse_openapi_endpoints(after);

    let before_keys: HashSet<String> = before_endpoints.keys().cloned().collect();
    let after_keys: HashSet<String> = after_endpoints.keys().cloned().collect();

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut modified = Vec::new();

    for k in after_keys.difference(&before_keys) {
        added.push(SchemaDiffEntry {
            name: k.clone(),
            kind: "endpoint".to_string(),
            change: "added".to_string(),
            detail: after_endpoints.get(k).cloned(),
        });
    }
    for k in before_keys.difference(&after_keys) {
        removed.push(SchemaDiffEntry {
            name: k.clone(),
            kind: "endpoint".to_string(),
            change: "removed".to_string(),
            detail: before_endpoints.get(k).cloned(),
        });
    }
    for k in before_keys.intersection(&after_keys) {
        if before_endpoints.get(k) != after_endpoints.get(k) {
            modified.push(SchemaDiffEntry {
                name: k.clone(),
                kind: "endpoint".to_string(),
                change: "modified".to_string(),
                detail: after_endpoints.get(k).cloned(),
            });
        }
    }

    added.sort_by(|a, b| a.name.cmp(&b.name));
    removed.sort_by(|a, b| a.name.cmp(&b.name));
    modified.sort_by(|a, b| a.name.cmp(&b.name));
    (added, removed, modified)
}

fn parse_openapi_endpoints(content: &str) -> HashMap<String, String> {
    let mut endpoints: HashMap<String, String> = HashMap::new();
    let methods = ["get:", "post:", "put:", "patch:", "delete:", "head:", "options:"];
    let mut current_path: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        // Path lines start at 2-space indent with /
        if line.starts_with("  /") || line.starts_with("\t/") {
            current_path = Some(trimmed.trim_end_matches(':').to_string());
        }
        if let Some(ref path) = current_path {
            for method in &methods {
                if trimmed == *method || trimmed.starts_with(method) {
                    let m = method.trim_end_matches(':');
                    let key = format!("{} {}", m.to_uppercase(), path);
                    endpoints.entry(key).or_insert_with(|| trimmed.to_string());
                }
            }
        }
    }
    endpoints
}

// ─── DB schema diff ──────────────────────────────────────────────────────────

fn diff_db_schema(
    before: &str,
    after: &str,
    schema_type: &str,
) -> (Vec<SchemaDiffEntry>, Vec<SchemaDiffEntry>, Vec<SchemaDiffEntry>) {
    let before_tables = parse_db_tables(before, schema_type);
    let after_tables = parse_db_tables(after, schema_type);

    let before_keys: HashSet<String> = before_tables.keys().cloned().collect();
    let after_keys: HashSet<String> = after_tables.keys().cloned().collect();

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut modified = Vec::new();

    for k in after_keys.difference(&before_keys) {
        added.push(SchemaDiffEntry {
            name: k.clone(),
            kind: "table".to_string(),
            change: "added".to_string(),
            detail: None,
        });
    }
    for k in before_keys.difference(&after_keys) {
        removed.push(SchemaDiffEntry {
            name: k.clone(),
            kind: "table".to_string(),
            change: "removed".to_string(),
            detail: None,
        });
    }
    for k in before_keys.intersection(&after_keys) {
        let b_fields = &before_tables[k];
        let a_fields = &after_tables[k];
        // Diff fields
        let b_set: HashSet<&String> = b_fields.iter().collect();
        let a_set: HashSet<&String> = a_fields.iter().collect();
        let field_added: Vec<String> = a_set.difference(&b_set).map(|s| s.to_string()).collect();
        let field_removed: Vec<String> = b_set.difference(&a_set).map(|s| s.to_string()).collect();
        for f in &field_added {
            modified.push(SchemaDiffEntry {
                name: format!("{}.{}", k, f),
                kind: "field".to_string(),
                change: "added".to_string(),
                detail: None,
            });
        }
        for f in &field_removed {
            modified.push(SchemaDiffEntry {
                name: format!("{}.{}", k, f),
                kind: "field".to_string(),
                change: "removed".to_string(),
                detail: None,
            });
        }
    }
    added.sort_by(|a, b| a.name.cmp(&b.name));
    removed.sort_by(|a, b| a.name.cmp(&b.name));
    modified.sort_by(|a, b| a.name.cmp(&b.name));
    (added, removed, modified)
}

fn parse_db_tables(content: &str, schema_type: &str) -> HashMap<String, Vec<String>> {
    let mut tables: HashMap<String, Vec<String>> = HashMap::new();
    let mut current: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if schema_type == "prisma" {
            if trimmed.starts_with("model ") || trimmed.starts_with("type ") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2 {
                    current = Some(parts[1].to_string());
                    tables.insert(parts[1].to_string(), Vec::new());
                }
            } else if trimmed == "}" {
                current = None;
            } else if let Some(ref name) = current {
                if !trimmed.is_empty() && !trimmed.starts_with("//") && !trimmed.starts_with("@@") {
                    let field_name = trimmed.split_whitespace().next().unwrap_or("").to_string();
                    if !field_name.is_empty() {
                        tables.entry(name.clone()).or_default().push(field_name);
                    }
                }
            }
        } else {
            // SQL
            let upper = trimmed.to_uppercase();
            if upper.starts_with("CREATE TABLE") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if let Some(name) = parts.get(2) {
                    let name = name.trim_matches('(').trim_matches('`').trim_matches('"').to_string();
                    current = Some(name.clone());
                    tables.insert(name, Vec::new());
                }
            } else if trimmed.starts_with(");") || trimmed == ");" {
                current = None;
            } else if let Some(ref name) = current {
                if !trimmed.is_empty() && !trimmed.starts_with("--") {
                    let col = trimmed.split_whitespace().next().unwrap_or("").trim_matches('`').trim_matches('"').to_string();
                    if !col.is_empty() && !col.to_uppercase().starts_with("PRIMARY") && !col.to_uppercase().starts_with("UNIQUE") && !col.to_uppercase().starts_with("INDEX") && !col.to_uppercase().starts_with("KEY") {
                        tables.entry(name.clone()).or_default().push(col);
                    }
                }
            }
        }
    }
    tables
}

// ─── TypeScript diff ─────────────────────────────────────────────────────────

fn diff_typescript(
    before: &str,
    after: &str,
) -> (Vec<SchemaDiffEntry>, Vec<SchemaDiffEntry>, Vec<SchemaDiffEntry>) {
    let before_types = parse_ts_exports(before);
    let after_types = parse_ts_exports(after);

    let before_keys: HashSet<String> = before_types.keys().cloned().collect();
    let after_keys: HashSet<String> = after_types.keys().cloned().collect();

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut modified = Vec::new();

    for k in after_keys.difference(&before_keys) {
        added.push(SchemaDiffEntry {
            name: k.clone(),
            kind: after_types[k].clone(),
            change: "added".to_string(),
            detail: None,
        });
    }
    for k in before_keys.difference(&after_keys) {
        removed.push(SchemaDiffEntry {
            name: k.clone(),
            kind: before_types[k].clone(),
            change: "removed".to_string(),
            detail: None,
        });
    }
    for k in before_keys.intersection(&after_keys) {
        if before_types.get(k) != after_types.get(k) {
            modified.push(SchemaDiffEntry {
                name: k.clone(),
                kind: after_types[k].clone(),
                change: "modified".to_string(),
                detail: None,
            });
        }
    }
    added.sort_by(|a, b| a.name.cmp(&b.name));
    removed.sort_by(|a, b| a.name.cmp(&b.name));
    modified.sort_by(|a, b| a.name.cmp(&b.name));
    (added, removed, modified)
}

fn parse_ts_exports(content: &str) -> HashMap<String, String> {
    let mut types: HashMap<String, String> = HashMap::new();
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with("export interface ") || t.starts_with("export abstract class ") || t.starts_with("export class ") || t.starts_with("export type ") || t.starts_with("export enum ") || t.starts_with("export function ") || t.starts_with("export const ") || t.starts_with("export async function ") {
            let parts: Vec<&str> = t.splitn(4, ' ').collect();
            // "export" "kind" "name" ...
            if parts.len() >= 3 {
                let kind = parts[1].to_string();
                let name = parts[2].trim_end_matches('{').trim_end_matches('(').trim_end_matches('=').trim().to_string();
                if !name.is_empty() {
                    types.insert(name, kind);
                }
            }
        }
    }
    types
}

// ─── Generic line diff (fallback) ────────────────────────────────────────────

fn diff_generic_lines(
    before: &str,
    after: &str,
) -> (Vec<SchemaDiffEntry>, Vec<SchemaDiffEntry>, Vec<SchemaDiffEntry>) {
    let before_lines: HashSet<&str> = before.lines().collect();
    let after_lines: HashSet<&str> = after.lines().collect();

    let added: Vec<SchemaDiffEntry> = after_lines
        .difference(&before_lines)
        .filter(|l| !l.trim().is_empty())
        .map(|l| SchemaDiffEntry {
            name: l.trim().chars().take(80).collect(),
            kind: "line".to_string(),
            change: "added".to_string(),
            detail: None,
        })
        .collect();

    let removed: Vec<SchemaDiffEntry> = before_lines
        .difference(&after_lines)
        .filter(|l| !l.trim().is_empty())
        .map(|l| SchemaDiffEntry {
            name: l.trim().chars().take(80).collect(),
            kind: "line".to_string(),
            change: "removed".to_string(),
            detail: None,
        })
        .collect();

    (added, removed, vec![])
}
