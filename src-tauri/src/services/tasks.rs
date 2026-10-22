//! Tasks: the local store and the queue of changes waiting to reach Notion.
//!
//! SQLite is the source of truth for the local list. Notion, once connected,
//! is the source of truth for the *shared* list — but the local store always
//! wins in the UI, so the app keeps working offline and shows exactly which
//! tasks have not been pushed yet.

use rusqlite::{params, Row};

use crate::db::{query_optional, Database};
use crate::error::{AppError, AppResult};
use crate::types::{now_ms, SyncOperation, Task, TaskSyncState};

const MAX_TITLE_LENGTH: usize = 500;
const MAX_DESCRIPTION_LENGTH: usize = 5000;

fn row_to_task(row: &Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get(0)?,
        notion_page_id: row.get(1)?,
        title: row.get(2)?,
        description: row.get(3)?,
        done: row.get::<_, i64>(4)? != 0,
        due_at: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        remote_updated_at: row.get(8)?,
        sync_state: TaskSyncState::from_str(&row.get::<_, String>(9)?),
        pending_ops: 0,
    })
}

const TASK_COLUMNS: &str = "id, notion_page_id, title, description, done, due_at, created_at, updated_at, remote_updated_at, sync_state";

pub fn list(db: &Database) -> AppResult<Vec<Task>> {
    let conn = db.lock()?;
    let mut stmt = conn.prepare(&format!(
        "SELECT {TASK_COLUMNS} FROM tasks ORDER BY done ASC, COALESCE(due_at, 9_999_999_999_999) ASC, created_at DESC"
    ))?;

    let mut tasks = stmt
        .query_map([], row_to_task)?
        .collect::<Result<Vec<_>, _>>()?;

    // The pending-operation count is a per-task aggregate; attaching it here
    // keeps the list query to one round trip.
    let mut stmt = conn.prepare(
        "SELECT task_id, COUNT(*) FROM sync_operations GROUP BY task_id",
    )?;
    let counts: std::collections::HashMap<String, u32> = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u32)))?
        .collect::<Result<_, _>>()?;

    for task in &mut tasks {
        task.pending_ops = counts.get(&task.id).copied().unwrap_or(0);
    }

    Ok(tasks)
}

pub fn get(db: &Database, id: &str) -> AppResult<Task> {
    let conn = db.lock()?;
    let task = query_optional(
        &conn,
        &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id = ?1"),
        &[&id],
        row_to_task,
    )?
    .ok_or_else(|| AppError::msg("That task no longer exists."))?;
    Ok(task)
}

pub fn find_by_notion_page(db: &Database, page_id: &str) -> AppResult<Option<Task>> {
    let conn = db.lock()?;
    Ok(query_optional(
        &conn,
        &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE notion_page_id = ?1"),
        &[&page_id],
        row_to_task,
    )?)
}

/// Validates and stores a new task.
pub fn create(
    db: &Database,
    title: &str,
    description: Option<String>,
    due_at: Option<i64>,
) -> AppResult<Task> {
    let title = validate_title(title)?;
    let description = validate_description(description)?;

    let now = now_ms();
    let id = uuid::Uuid::new_v4().to_string();
    let conn = db.lock()?;
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO tasks (id, notion_page_id, title, description, done, due_at, created_at, updated_at, sync_state)
         VALUES (?1, NULL, ?2, ?3, 0, ?4, ?5, ?5, 'local')",
        params![id, title, description, due_at, now],
    )?;
    tx.commit()?;

    drop(conn);
    let mut task = get(db, &id)?;
    // A task created locally is immediately a change that Notion has not seen.
    enqueue(db, &task.id, "create", &serde_json::json!({ "title": title, "description": description, "due_at": due_at }))?;
    task = get(db, &id)?;
    Ok(task)
}

/// Applies a partial update to a task and queues the change.
pub fn update(
    db: &Database,
    id: &str,
    patch: &serde_json::Value,
) -> AppResult<Task> {
    let existing = get(db, id)?;

    let title = match patch.get("title") {
        Some(value) => validate_title(value.as_str().unwrap_or_default())?,
        None => existing.title.clone(),
    };
    let description = match patch.get("description") {
        Some(serde_json::Value::Null) => None,
        Some(value) => validate_description(Some(value.as_str().unwrap_or_default().to_string()))?,
        None => existing.description.clone(),
    };
    let done = match patch.get("done") {
        Some(value) => value.as_bool().unwrap_or(existing.done),
        None => existing.done,
    };
    let due_at = match patch.get("dueAt").or_else(|| patch.get("due_at")) {
        Some(serde_json::Value::Null) => None,
        Some(value) => value.as_i64(),
        None => existing.due_at,
    };

    let now = now_ms();
    let conn = db.lock()?;
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE tasks SET title = ?2, description = ?3, done = ?4, due_at = ?5, updated_at = ?6, sync_state = 'pending'
         WHERE id = ?1",
        params![id, title, description, done as i64, due_at, now],
    )?;
    tx.commit()?;
    drop(conn);

    enqueue(
        db,
        id,
        "update",
        &serde_json::json!({
            "title": title,
            "description": description,
            "done": done,
            "due_at": due_at,
        }),
    )?;

    get(db, id)
}

pub fn delete(db: &Database, id: &str) -> AppResult<()> {
    let existing = get(db, id)?;
    let conn = db.lock()?;
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM sync_operations WHERE task_id = ?1", params![id])?;
    tx.execute("DELETE FROM tasks WHERE id = ?1", params![id])?;
    tx.commit()?;
    drop(conn);

    // A task that was never pushed to Notion needs no remote delete.
    if let Some(page_id) = existing.notion_page_id {
        enqueue(
            db,
            &id,
            "delete",
            &serde_json::json!({ "notion_page_id": page_id }),
        )?;
    }
    Ok(())
}

/* -------------------------------------------------------------------------- */
/* Sync queue                                                                  */
/* -------------------------------------------------------------------------- */

/// Records a pending operation for a task.
///
/// Operations are appended, never merged: if the user renames a task twice
/// while offline, both changes are replayed in order and the final state is
/// the one the user last saw. Merging would risk dropping a change.
pub fn enqueue(db: &Database, task_id: &str, kind: &str, payload: &serde_json::Value) -> AppResult<()> {
    let conn = db.lock()?;
    conn.execute(
        "INSERT INTO sync_operations (task_id, kind, payload, attempts, created_at)
         VALUES (?1, ?2, ?3, 0, ?4)",
        params![task_id, kind, serde_json::to_string(payload)?, now_ms()],
    )?;
    conn.execute(
        "UPDATE tasks SET sync_state = 'pending' WHERE id = ?1",
        params![task_id],
    )?;
    Ok(())
}

/// The operations waiting to be sent, oldest first.
pub fn pending_operations(db: &Database) -> AppResult<Vec<SyncOperation>> {
    let conn = db.lock()?;
    let mut stmt = conn.prepare(
        "SELECT id, task_id, kind, payload, attempts, last_error, created_at
         FROM sync_operations ORDER BY id ASC",
    )?;
    let ops = stmt
        .query_map([], |row| {
            Ok(SyncOperation {
                id: row.get(0)?,
                task_id: row.get(1)?,
                kind: row.get(2)?,
                payload: row.get(3)?,
                attempts: row.get::<_, i64>(4)? as u32,
                last_error: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ops)
}

pub fn pending_count(db: &Database) -> AppResult<u32> {
    let conn = db.lock()?;
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM sync_operations", [], |row| row.get(0))?;
    Ok(count as u32)
}

/// Removes a completed operation and marks the task as in sync.
pub fn complete_operation(db: &Database, op_id: i64, task_id: &str, notion_page_id: Option<&str>) -> AppResult<()> {
    let conn = db.lock()?;
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM sync_operations WHERE id = ?1", params![op_id])?;

    if let Some(page_id) = notion_page_id {
        tx.execute(
            "UPDATE tasks SET notion_page_id = ?2 WHERE id = ?1",
            params![task_id, page_id],
        )?;
    }

    // The task is only "synced" when nothing else is queued for it.
    let remaining: i64 = tx.query_row(
        "SELECT COUNT(*) FROM sync_operations WHERE task_id = ?1",
        params![task_id],
        |row| row.get(0),
    )?;
    if remaining == 0 {
        tx.execute(
            "UPDATE tasks SET sync_state = 'synced' WHERE id = ?1",
            params![task_id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Records a failed attempt. The operation stays queued.
pub fn fail_operation(db: &Database, op_id: i64, task_id: &str, message: &str) -> AppResult<()> {
    let conn = db.lock()?;
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE sync_operations SET attempts = attempts + 1, last_error = ?2 WHERE id = ?1",
        params![op_id, message],
    )?;
    tx.execute(
        "UPDATE tasks SET sync_state = 'failed' WHERE id = ?1",
        params![task_id],
    )?;
    tx.commit()?;
    Ok(())
}

/// Marks a task as conflicting: both sides changed since the last sync.
pub fn mark_conflict(db: &Database, task_id: &str) -> AppResult<()> {
    let conn = db.lock()?;
    conn.execute(
        "UPDATE tasks SET sync_state = 'conflict' WHERE id = ?1",
        params![task_id],
    )?;
    Ok(())
}

/// Overwrites the local task with the remote version, used when a remote
/// change wins a conflict.
pub fn apply_remote(
    db: &Database,
    local_id: &str,
    notion_page_id: &str,
    title: &str,
    description: Option<String>,
    done: bool,
    due_at: Option<i64>,
    remote_updated_at: i64,
) -> AppResult<()> {
    let conn = db.lock()?;
    conn.execute(
        "UPDATE tasks SET notion_page_id = ?2, title = ?3, description = ?4, done = ?5, due_at = ?6,
            remote_updated_at = ?7, updated_at = ?8, sync_state = 'synced'
         WHERE id = ?1",
        params![local_id, notion_page_id, title, description, done as i64, due_at, remote_updated_at, now_ms()],
    )?;
    Ok(())
}

/// Inserts a task that only exists in Notion.
pub fn insert_remote(
    db: &Database,
    notion_page_id: &str,
    title: &str,
    description: Option<String>,
    done: bool,
    due_at: Option<i64>,
    remote_updated_at: i64,
) -> AppResult<()> {
    let now = now_ms();
    let conn = db.lock()?;
    conn.execute(
        "INSERT INTO tasks (id, notion_page_id, title, description, done, due_at, created_at, updated_at, remote_updated_at, sync_state)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8, 'synced')",
        params![
            uuid::Uuid::new_v4().to_string(),
            notion_page_id,
            title,
            description,
            done as i64,
            due_at,
            now,
            remote_updated_at
        ],
    )?;
    Ok(())
}

/// True when the local copy changed after the last known remote state.
pub fn has_local_changes(task: &Task) -> bool {
    match task.remote_updated_at {
        Some(remote) => task.updated_at > remote,
        None => task.notion_page_id.is_none(),
    }
}

fn validate_title(title: &str) -> AppResult<String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err(AppError::msg("Give the task a title."));
    }
    if trimmed.chars().count() > MAX_TITLE_LENGTH {
        return Err(AppError::msg(format!(
            "The title is too long. Keep it under {MAX_TITLE_LENGTH} characters."
        )));
    }
    Ok(trimmed.to_string())
}

fn validate_description(description: Option<String>) -> AppResult<Option<String>> {
    match description {
        None => Ok(None),
        Some(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            if trimmed.chars().count() > MAX_DESCRIPTION_LENGTH {
                return Err(AppError::msg(format!(
                    "The description is too long. Keep it under {MAX_DESCRIPTION_LENGTH} characters."
                )));
            }
            Ok(Some(trimmed.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Database {
        Database::open_in_memory().unwrap()
    }

    #[test]
    fn a_task_is_created_and_queued_for_sync() {
        let db = db();
        let task = create(&db, "  Write the report  ", None, None).unwrap();

        assert_eq!(task.title, "Write the report");
        assert!(!task.done);
        assert_eq!(task.sync_state, TaskSyncState::Pending);
        assert_eq!(pending_count(&db).unwrap(), 1);
    }

    #[test]
    fn an_empty_title_is_rejected() {
        let db = db();
        assert!(create(&db, "   ", None, None).is_err());
    }

    #[test]
    fn an_over_long_title_is_rejected() {
        let db = db();
        let long = "x".repeat(MAX_TITLE_LENGTH + 1);
        assert!(create(&db, &long, None, None).is_err());
    }

    #[test]
    fn updating_a_task_queues_another_operation() {
        let db = db();
        let task = create(&db, "Task", None, None).unwrap();
        let updated = update(&db, &task.id, &serde_json::json!({ "done": true })).unwrap();

        assert!(updated.done);
        assert_eq!(pending_count(&db).unwrap(), 2);
    }

    #[test]
    fn completing_the_queue_marks_the_task_as_synced() {
        let db = db();
        let task = create(&db, "Task", None, None).unwrap();
        let ops = pending_operations(&db).unwrap();

        for op in &ops {
            complete_operation(&db, op.id, &task.id, Some("page-123")).unwrap();
        }

        assert_eq!(pending_count(&db).unwrap(), 0);
        let reloaded = get(&db, &task.id).unwrap();
        assert_eq!(reloaded.sync_state, TaskSyncState::Synced);
        assert_eq!(reloaded.notion_page_id.as_deref(), Some("page-123"));
    }

    #[test]
    fn a_failed_operation_stays_queued() {
        let db = db();
        let task = create(&db, "Task", None, None).unwrap();
        let op = pending_operations(&db).unwrap()[0].clone();

        fail_operation(&db, op.id, &task.id, "network unreachable").unwrap();

        assert_eq!(pending_count(&db).unwrap(), 1);
        let reloaded = get(&db, &task.id).unwrap();
        assert_eq!(reloaded.sync_state, TaskSyncState::Failed);
    }

    #[test]
    fn deleting_a_task_removes_its_operations() {
        let db = db();
        let task = create(&db, "Task", None, None).unwrap();
        delete(&db, &task.id).unwrap();
        assert_eq!(pending_count(&db).unwrap(), 0);
        assert!(get(&db, &task.id).is_err());
    }

    #[test]
    fn tasks_are_listed_with_their_pending_counts() {
        let db = db();
        create(&db, "One", None, None).unwrap();
        create(&db, "Two", None, None).unwrap();

        let tasks = list(&db).unwrap();
        assert_eq!(tasks.len(), 2);
        assert!(tasks.iter().all(|t| t.pending_ops == 1));
    }

    #[test]
    fn local_changes_are_detected_against_the_remote_state() {
        let mut task = Task {
            id: "a".into(),
            notion_page_id: Some("page".into()),
            title: "Task".into(),
            description: None,
            done: false,
            due_at: None,
            created_at: 100,
            updated_at: 200,
            remote_updated_at: Some(150),
            sync_state: TaskSyncState::Synced,
            pending_ops: 0,
        };
        assert!(has_local_changes(&task));

        task.updated_at = 120;
        assert!(!has_local_changes(&task));
    }
}
