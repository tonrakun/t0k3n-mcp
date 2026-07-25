use anyhow::Result;
use chrono::Utc;
use rusqlite::params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::server::db::Database;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MemorySaveParams {
    #[schemars(description = "Unique key for the memory")]
    pub key: String,
    #[schemars(description = "Value to store")]
    pub value: String,
    #[schemars(description = "Optional tags for filtering")]
    pub tags: Option<Vec<String>>,
}

pub fn memory_save(db: &Database, params: MemorySaveParams) -> Result<String> {
    let now = Utc::now().timestamp();
    let tags = serde_json::to_string(&params.tags.unwrap_or_default())?;
    db.conn.execute(
        "INSERT INTO memories (key, value, tags, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value, tags=excluded.tags, updated_at=excluded.updated_at",
        params![params.key, params.value, tags, now],
    )?;
    Ok(format!("Memory '{}' saved.", params.key))
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MemoryGetParams {
    #[schemars(description = "Key to retrieve")]
    pub key: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: i64,
    pub key: String,
    pub value: String,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

pub fn memory_get(db: &Database, params: MemoryGetParams) -> Result<Option<MemoryEntry>> {
    let mut stmt = db.conn.prepare(
        "SELECT id, key, value, tags, created_at, updated_at FROM memories WHERE key = ?1",
    )?;
    let entry = stmt
        .query_row(params![params.key], |row| {
            let tags_str: String = row.get(3)?;
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                tags_str,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })
        .optional()?;

    Ok(
        entry.map(|(id, key, value, tags_str, created_at, updated_at)| {
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
            MemoryEntry {
                id,
                key,
                value,
                tags,
                created_at,
                updated_at,
            }
        }),
    )
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MemoryListParams {
    #[schemars(description = "Filter by tag")]
    pub tag: Option<String>,
    #[schemars(description = "Filter by keyword in key or value")]
    pub search: Option<String>,
}

pub fn memory_list(db: &Database, params: MemoryListParams) -> Result<Vec<MemoryEntry>> {
    let mut stmt = db.conn.prepare(
        "SELECT id, key, value, tags, created_at, updated_at FROM memories ORDER BY updated_at DESC",
    )?;

    let entries: Vec<MemoryEntry> = stmt
        .query_map([], |row| {
            let tags_str: String = row.get(3)?;
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                tags_str,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .map(|(id, key, value, tags_str, created_at, updated_at)| {
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
            MemoryEntry {
                id,
                key,
                value,
                tags,
                created_at,
                updated_at,
            }
        })
        .filter(|e| {
            if let Some(tag) = &params.tag
                && !e.tags.contains(tag)
            {
                return false;
            }
            if let Some(search) = &params.search {
                let s = search.to_lowercase();
                if !e.key.to_lowercase().contains(&s) && !e.value.to_lowercase().contains(&s) {
                    return false;
                }
            }
            true
        })
        .collect();

    Ok(entries)
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MemoryDeleteParams {
    #[schemars(description = "Key to delete")]
    pub key: String,
}

pub fn memory_delete(db: &Database, params: MemoryDeleteParams) -> Result<String> {
    let n = db
        .conn
        .execute("DELETE FROM memories WHERE key = ?1", params![params.key])?;
    if n > 0 {
        Ok(format!("Memory '{}' deleted.", params.key))
    } else {
        Ok(format!("Memory '{}' not found.", params.key))
    }
}

trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for std::result::Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
