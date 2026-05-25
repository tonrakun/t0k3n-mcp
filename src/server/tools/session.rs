use anyhow::Result;
use chrono::Utc;
use rusqlite::params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::server::db::Database;

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionEntry {
    pub id: i64,
    pub name: String,
    pub snapshot: Value,
    pub created_at: i64,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SessionSnapshotParams {
    #[schemars(description = "Name for this snapshot")]
    pub name: String,
    #[schemars(description = "Any JSON data to snapshot (workspace state, task context, etc.)")]
    pub data: Value,
}

pub fn session_snapshot(db: &Database, params: SessionSnapshotParams) -> Result<SessionEntry> {
    let now = Utc::now().timestamp();
    let snapshot = serde_json::to_string(&params.data)?;
    db.conn.execute(
        "INSERT INTO sessions (name, snapshot, created_at) VALUES (?1, ?2, ?3)",
        params![params.name, snapshot, now],
    )?;
    let id = db.conn.last_insert_rowid();
    Ok(SessionEntry {
        id,
        name: params.name,
        snapshot: params.data,
        created_at: now,
    })
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SessionRestoreParams {
    #[schemars(description = "Session ID to restore")]
    pub id: i64,
}

pub fn session_restore(db: &Database, params: SessionRestoreParams) -> Result<SessionEntry> {
    let (id, name, snapshot_str, created_at) = db.conn.query_row(
        "SELECT id, name, snapshot, created_at FROM sessions WHERE id = ?1",
        params![params.id],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, i64>(3)?)),
    )?;
    let snapshot: Value = serde_json::from_str(&snapshot_str).unwrap_or(Value::Null);
    Ok(SessionEntry { id, name, snapshot, created_at })
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SessionListParams {
    #[schemars(description = "Maximum number of sessions to return (default: 20)")]
    pub limit: Option<i64>,
}

pub fn session_list(db: &Database, params: SessionListParams) -> Result<Vec<SessionEntry>> {
    let limit = params.limit.unwrap_or(20);
    let mut stmt = db.conn.prepare(
        "SELECT id, name, snapshot, created_at FROM sessions ORDER BY created_at DESC LIMIT ?1",
    )?;
    let sessions = stmt
        .query_map(params![limit], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, i64>(3)?))
        })?
        .filter_map(|r| r.ok())
        .map(|(id, name, snapshot_str, created_at)| {
            let snapshot: Value = serde_json::from_str(&snapshot_str).unwrap_or(Value::Null);
            SessionEntry { id, name, snapshot, created_at }
        })
        .collect();
    Ok(sessions)
}
