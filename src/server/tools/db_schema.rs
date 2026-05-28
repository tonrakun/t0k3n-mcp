use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::security::safe_path;
use super::fs::estimate_tokens;

// ─── read_db_schema ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadDbSchemaParams {
    #[schemars(description = "Root-relative path to a .prisma or .sql file. Omit to auto-detect (searches *.prisma, then *.sql under workspace root, depth ≤ 4).")]
    pub path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DbTableEntry {
    pub name: String,
    pub kind: String, // "model" | "table" | "enum" | "type"
    pub field_count: usize,
}

#[derive(Debug, Serialize)]
pub struct ReadDbSchemaResult {
    pub path: String,
    pub format: String, // "prisma" | "sql"
    pub tables: Vec<DbTableEntry>,
    pub token_count: usize,
}

pub fn read_db_schema(root: &Path, params: ReadDbSchemaParams) -> anyhow::Result<ReadDbSchemaResult> {
    let file_path = if let Some(ref p) = params.path {
        safe_path(root, p)?
    } else {
        auto_detect_db_file(root)?
    };

    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| anyhow::anyhow!("ファイル読み取り失敗: {e}"))?;
    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let rel = file_path.strip_prefix(root).unwrap_or(&file_path)
        .to_string_lossy().replace('\\', "/");

    let (format, tables) = if ext == "prisma" {
        ("prisma".to_string(), parse_prisma_schema(&content))
    } else {
        ("sql".to_string(), parse_sql_schema(&content))
    };

    let json = serde_json::to_string(&tables).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadDbSchemaResult { path: rel, format, tables, token_count })
}

fn auto_detect_db_file(root: &Path) -> anyhow::Result<PathBuf> {
    let mut sql_candidate: Option<PathBuf> = None;

    for entry in WalkBuilder::new(root)
        .max_depth(Some(4))
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .build()
        .flatten()
    {
        let path = entry.path().to_path_buf();
        if !path.is_file() { continue; }
        match path.extension().and_then(|e| e.to_str()) {
            Some("prisma") => return Ok(path),
            Some("sql") if sql_candidate.is_none() => { sql_candidate = Some(path); }
            _ => {}
        }
    }

    sql_candidate.ok_or_else(|| anyhow::anyhow!(
        "スキーマファイルが見つかりません (.prisma / .sql)。path パラメータで明示指定してください。"
    ))
}

fn parse_prisma_schema(content: &str) -> Vec<DbTableEntry> {
    let re = Regex::new(r"(?m)^(model|enum|type)\s+(\w+)\s*\{([^}]*)\}").unwrap();
    let mut tables = Vec::new();

    for cap in re.captures_iter(content) {
        let kind = cap[1].to_string();
        let name = cap[2].to_string();
        let body = &cap[3];
        let field_count = body.lines()
            .filter(|l| {
                let t = l.trim();
                !t.is_empty() && !t.starts_with("//") && !t.starts_with("@@")
            })
            .count();
        tables.push(DbTableEntry { name, kind, field_count });
    }
    tables
}

fn parse_sql_schema(content: &str) -> Vec<DbTableEntry> {
    let re = Regex::new(
        r#"(?ims)CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?[`"\[]?(\w+)[`"\]]?\s*\(([^;]*?)\)\s*;"#
    ).unwrap();
    let mut tables = Vec::new();

    for cap in re.captures_iter(content) {
        let name = cap[1].to_string();
        let body = &cap[2];
        let field_count = body.lines()
            .filter(|l| {
                let t = l.trim();
                if t.is_empty() || t.starts_with("--") { return false; }
                let u = t.to_uppercase();
                !u.starts_with("PRIMARY KEY") && !u.starts_with("UNIQUE")
                    && !u.starts_with("FOREIGN KEY") && !u.starts_with("INDEX")
                    && !u.starts_with("KEY ") && !u.starts_with("CONSTRAINT")
                    && !u.starts_with("CHECK")
            })
            .count();
        tables.push(DbTableEntry { name, kind: "table".to_string(), field_count });
    }
    tables
}

// ─── read_db_table ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadDbTableParams {
    #[schemars(description = "Root-relative path to the .prisma or .sql file (from read_db_schema result).")]
    pub path: String,
    #[schemars(description = "Table or model name to retrieve (from read_db_schema tables list).")]
    pub table: String,
}

#[derive(Debug, Serialize)]
pub struct DbFieldEntry {
    pub name: String,
    pub field_type: String,
    pub attributes: String,
}

#[derive(Debug, Serialize)]
pub struct ReadDbTableResult {
    pub name: String,
    pub kind: String,
    pub fields: Vec<DbFieldEntry>,
    pub token_count: usize,
}

pub fn read_db_table(root: &Path, params: ReadDbTableParams) -> anyhow::Result<ReadDbTableResult> {
    let file_path = safe_path(root, &params.path)?;
    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| anyhow::anyhow!("ファイル読み取り失敗: {e}"))?;
    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    if ext == "prisma" {
        parse_prisma_table(&content, &params.table)
    } else {
        parse_sql_table(&content, &params.table)
    }
}

fn parse_prisma_table(content: &str, table: &str) -> anyhow::Result<ReadDbTableResult> {
    let pattern = format!(
        r"(?m)^(model|enum|type)\s+{}\s*\{{([^}}]*)\}}",
        regex::escape(table)
    );
    let re = Regex::new(&pattern).unwrap();

    let cap = re.captures(content)
        .ok_or_else(|| anyhow::anyhow!("テーブル/モデル '{}' が見つかりません", table))?;

    let kind = cap[1].to_string();
    let body = &cap[2];
    let mut fields = Vec::new();

    for line in body.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("//") || t.starts_with("@@") { continue; }

        let parts: Vec<&str> = t.splitn(3, char::is_whitespace).collect();
        if parts.len() >= 2 {
            let name = parts[0].to_string();
            let field_type = parts[1].to_string();
            let attributes = parts.get(2).unwrap_or(&"").trim().to_string();
            fields.push(DbFieldEntry { name, field_type, attributes });
        }
    }

    let json = serde_json::to_string(&fields).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadDbTableResult { name: table.to_string(), kind, fields, token_count })
}

fn parse_sql_table(content: &str, table: &str) -> anyhow::Result<ReadDbTableResult> {
    let pattern = format!(
        r#"(?ims)CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?[`"\[]?{0}[`"\]]?\s*\(([^;]*?)\)\s*;"#,
        regex::escape(table)
    );
    let re = Regex::new(&pattern).unwrap();

    let cap = re.captures(content)
        .ok_or_else(|| anyhow::anyhow!("テーブル '{}' が見つかりません", table))?;

    let body = &cap[1];
    let mut fields = Vec::new();

    for line in body.lines() {
        let t = line.trim().trim_end_matches(',').trim();
        if t.is_empty() || t.starts_with("--") { continue; }
        let u = t.to_uppercase();
        if u.starts_with("PRIMARY KEY") || u.starts_with("UNIQUE") || u.starts_with("FOREIGN KEY")
            || u.starts_with("INDEX") || u.starts_with("KEY ") || u.starts_with("CONSTRAINT")
            || u.starts_with("CHECK") { continue; }

        let parts: Vec<&str> = t.splitn(3, char::is_whitespace).collect();
        if parts.len() >= 2 {
            let name = parts[0].trim_matches(|c| matches!(c, '`' | '"' | '[' | ']')).to_string();
            let field_type = parts[1].to_string();
            let attributes = parts.get(2).unwrap_or(&"").trim().to_string();
            fields.push(DbFieldEntry { name, field_type, attributes });
        }
    }

    let json = serde_json::to_string(&fields).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadDbTableResult { name: table.to_string(), kind: "table".to_string(), fields, token_count })
}
