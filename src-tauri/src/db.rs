//! SQLite access layer.
//!
//! One connection, guarded by a mutex, owned by the Tauri state. SQLite is
//! fast enough locally that a single serialised connection is simpler — and
//! safer — than a pool, and it keeps every write transactional by default.
//!
//! # Schema policy
//!
//! * **Structural changes** (new table, new column, new index) are migrations:
//!   numbered, applied in order, each in its own transaction, recorded in
//!   `schema_migrations`.
//! * **User settings** live in `app_settings` as one JSON value per key, so
//!   adding a setting is a code change rather than a migration.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

use crate::error::{AppError, AppResult};

/// Every migration, in order. Never edit a shipped migration — add a new one.
const MIGRATIONS: &[(i64, &str)] = &[
    (
        1,
        r#"
        CREATE TABLE app_settings (
            key         TEXT PRIMARY KEY,
            value       TEXT NOT NULL,
            updated_at  INTEGER NOT NULL
        );

        CREATE TABLE water_intake_records (
            id          TEXT PRIMARY KEY,
            amount_ml   INTEGER NOT NULL,
            logged_at   INTEGER NOT NULL,
            local_date  TEXT NOT NULL
        );
        CREATE INDEX idx_water_date ON water_intake_records(local_date);

        CREATE TABLE pomodoro_sessions (
            id          TEXT PRIMARY KEY,
            mode        TEXT NOT NULL,
            started_at  INTEGER NOT NULL,
            ended_at    INTEGER NOT NULL,
            completed   INTEGER NOT NULL DEFAULT 1,
            local_date  TEXT NOT NULL
        );
        CREATE INDEX idx_pomodoro_date ON pomodoro_sessions(local_date);

        CREATE TABLE pomodoro_state (
            id                      INTEGER PRIMARY KEY CHECK (id = 1),
            mode                    TEXT NOT NULL,
            status                  TEXT NOT NULL,
            ends_at                 INTEGER,
            remaining_seconds       INTEGER,
            completed_focus_sessions INTEGER NOT NULL DEFAULT 0,
            started_at              INTEGER
        );

        CREATE TABLE tasks (
            id                TEXT PRIMARY KEY,
            notion_page_id    TEXT,
            title             TEXT NOT NULL,
            description       TEXT,
            done              INTEGER NOT NULL DEFAULT 0,
            due_at            INTEGER,
            created_at        INTEGER NOT NULL,
            updated_at        INTEGER NOT NULL,
            remote_updated_at INTEGER,
            sync_state        TEXT NOT NULL DEFAULT 'local'
        );
        CREATE INDEX idx_tasks_notion_page ON tasks(notion_page_id);
        CREATE INDEX idx_tasks_updated ON tasks(updated_at);

        CREATE TABLE sync_operations (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id     TEXT NOT NULL,
            kind        TEXT NOT NULL,
            payload     TEXT NOT NULL,
            attempts    INTEGER NOT NULL DEFAULT 0,
            last_error  TEXT,
            created_at  INTEGER NOT NULL
        );
        CREATE INDEX idx_sync_task ON sync_operations(task_id);

        CREATE TABLE lyrics_cache (
            track_id    TEXT PRIMARY KEY,
            title       TEXT NOT NULL,
            artist      TEXT NOT NULL,
            synced      INTEGER NOT NULL DEFAULT 0,
            lines       TEXT NOT NULL,
            source_url  TEXT,
            fetched_at  INTEGER NOT NULL
        );
        "#,
    ),
];

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Opens (creating if needed) the database and applies pending migrations.
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;

        let db = Database {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    /// Opens an in-memory database. Used by the tests.
    #[cfg(test)]
    pub fn open_in_memory() -> AppResult<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Database {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version    INTEGER PRIMARY KEY,
                applied_at INTEGER NOT NULL
            );",
        )?;

        let current: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        for (version, sql) in MIGRATIONS {
            if *version <= current {
                continue;
            }
            // Each migration is atomic: it either applies completely or not
            // at all, so a half-migrated database can never be observed.
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                rusqlite::params![version, crate::types::now_ms()],
            )?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn lock(&self) -> AppResult<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| AppError::msg("The database is busy. Try again in a moment."))
    }

    /// The current schema version. Exposed for diagnostics.
    pub fn schema_version(&self) -> AppResult<i64> {
        let conn = self.lock()?;
        Ok(conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0))
    }
}

/// Reads a single optional value from a query.
pub fn query_optional<T, F>(conn: &Connection, sql: &str, params: &[&dyn rusqlite::ToSql], f: F) -> AppResult<Option<T>>
where
    F: FnOnce(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
{
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query(params)?;
    match rows.next()? {
        Some(row) => Ok(Some(f(row)?)),
        None => Ok(None),
    }
}

pub fn query_one<T, F>(conn: &Connection, sql: &str, params: &[&dyn rusqlite::ToSql], f: F) -> AppResult<T>
where
    F: FnOnce(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
{
    query_optional(conn, sql, params, f)?.ok_or_else(|| AppError::msg("The record was not found."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_apply_and_are_idempotent() {
        let db = Database::open_in_memory().unwrap();
        assert_eq!(db.schema_version().unwrap(), 1);

        // Re-running the migration pass must not fail or change the version.
        db.migrate().unwrap();
        assert_eq!(db.schema_version().unwrap(), 1);
    }

    #[test]
    fn expected_tables_exist() {
        let db = Database::open_in_memory().unwrap();
        let conn = db.lock().unwrap();
        for table in [
            "app_settings",
            "water_intake_records",
            "pomodoro_sessions",
            "pomodoro_state",
            "tasks",
            "sync_operations",
            "lyrics_cache",
        ] {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    rusqlite::params![table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "table {table} is missing");
        }
    }
}
