use anyhow::Result;
use chrono::Utc;
use rusqlite::params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::server::db::Database;

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskEntry {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: i64,
    pub due_date: Option<String>,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

fn row_to_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskEntry> {
    let tags_str: String = row.get(6)?;
    let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
    Ok(TaskEntry {
        id: row.get(0)?,
        title: row.get(1)?,
        description: row.get(2)?,
        status: row.get(3)?,
        priority: row.get(4)?,
        due_date: row.get(5)?,
        tags,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TaskCreateParams {
    #[schemars(description = "Task title")]
    pub title: String,
    #[schemars(description = "Task description (optional)")]
    pub description: Option<String>,
    #[schemars(description = "Status: pending, in_progress, done, cancelled (default: pending)")]
    pub status: Option<String>,
    #[schemars(description = "Priority 0-10 (default: 0)")]
    pub priority: Option<i64>,
    #[schemars(description = "Due date (ISO 8601 or any string)")]
    pub due_date: Option<String>,
    #[schemars(description = "Tags for filtering")]
    pub tags: Option<Vec<String>>,
}

pub fn task_create(db: &Database, params: TaskCreateParams) -> Result<TaskEntry> {
    let now = Utc::now().timestamp();
    let status = params.status.unwrap_or_else(|| "pending".to_string());
    let description = params.description.unwrap_or_default();
    let priority = params.priority.unwrap_or(0);
    let tags = serde_json::to_string(&params.tags.unwrap_or_default())?;

    db.conn.execute(
        "INSERT INTO tasks (title, description, status, priority, due_date, tags, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
        params![params.title, description, status, priority, params.due_date, tags, now],
    )?;
    let id = db.conn.last_insert_rowid();
    task_get_by_id(db, id)
}

fn task_get_by_id(db: &Database, id: i64) -> Result<TaskEntry> {
    let task = db.conn.query_row(
        "SELECT id, title, description, status, priority, due_date, tags, created_at, updated_at FROM tasks WHERE id = ?1",
        params![id],
        row_to_task,
    )?;
    Ok(task)
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TaskGetParams {
    #[schemars(description = "Task ID")]
    pub id: i64,
}

pub fn task_get(db: &Database, params: TaskGetParams) -> Result<TaskEntry> {
    task_get_by_id(db, params.id)
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TaskUpdateParams {
    #[schemars(description = "Task ID to update")]
    pub id: i64,
    #[schemars(description = "New title (optional)")]
    pub title: Option<String>,
    #[schemars(description = "New description (optional)")]
    pub description: Option<String>,
    #[schemars(description = "New status: pending, in_progress, done, cancelled")]
    pub status: Option<String>,
    #[schemars(description = "New priority 0-10")]
    pub priority: Option<i64>,
    #[schemars(description = "New due date")]
    pub due_date: Option<String>,
    #[schemars(description = "New tags")]
    pub tags: Option<Vec<String>>,
}

pub fn task_update(db: &Database, params: TaskUpdateParams) -> Result<TaskEntry> {
    let now = Utc::now().timestamp();
    let existing = task_get_by_id(db, params.id)?;

    let title = params.title.unwrap_or(existing.title);
    let description = params.description.unwrap_or(existing.description);
    let status = params.status.unwrap_or(existing.status);
    let priority = params.priority.unwrap_or(existing.priority);
    let due_date = params.due_date.or(existing.due_date);
    let tags = serde_json::to_string(&params.tags.unwrap_or(existing.tags))?;

    db.conn.execute(
        "UPDATE tasks SET title=?1, description=?2, status=?3, priority=?4, due_date=?5, tags=?6, updated_at=?7 WHERE id=?8",
        params![title, description, status, priority, due_date, tags, now, params.id],
    )?;
    task_get_by_id(db, params.id)
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TaskListParams {
    #[schemars(description = "Filter by status: pending, in_progress, done, cancelled")]
    pub status: Option<String>,
    #[schemars(description = "Filter by tag")]
    pub tag: Option<String>,
}

pub fn task_list(db: &Database, params: TaskListParams) -> Result<Vec<TaskEntry>> {
    let mut stmt = db.conn.prepare(
        "SELECT id, title, description, status, priority, due_date, tags, created_at, updated_at FROM tasks ORDER BY priority DESC, created_at DESC",
    )?;

    let tasks: Vec<TaskEntry> = stmt
        .query_map([], row_to_task)?
        .filter_map(|r| r.ok())
        .filter(|t| {
            if let Some(s) = &params.status {
                if &t.status != s {
                    return false;
                }
            }
            if let Some(tag) = &params.tag {
                if !t.tags.contains(tag) {
                    return false;
                }
            }
            true
        })
        .collect();

    Ok(tasks)
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct TaskDeleteParams {
    #[schemars(description = "Task ID to delete")]
    pub id: i64,
}

pub fn task_delete(db: &Database, params: TaskDeleteParams) -> Result<String> {
    let n = db.conn.execute("DELETE FROM tasks WHERE id = ?1", params![params.id])?;
    if n > 0 {
        Ok(format!("Task {} deleted.", params.id))
    } else {
        Ok(format!("Task {} not found.", params.id))
    }
}
