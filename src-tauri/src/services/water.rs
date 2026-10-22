//! Water intake tracking.
//!
//! Records are stored with the local calendar date they were logged on, so a
//! daily total is a simple indexed query rather than a range scan over
//! timestamps.

use rusqlite::params;

use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::types::{now_ms, WaterDay, WaterEntry};

/// A single serving, in millilitres. Values outside this range are rejected
/// rather than clamped: a 40 000 ml entry is a typo, and silently turning it
/// into 200 ml would hide the mistake.
const MIN_ENTRY_ML: u32 = 10;
const MAX_ENTRY_ML: u32 = 2000;

pub fn get_day(db: &Database, date: Option<String>, target_ml: u32) -> AppResult<WaterDay> {
    let date = date.unwrap_or_else(crate::services::today);
    let conn = db.lock()?;

    let mut stmt = conn.prepare(
        "SELECT id, amount_ml, logged_at FROM water_intake_records
         WHERE local_date = ?1 ORDER BY logged_at ASC",
    )?;
    let entries = stmt
        .query_map(params![date], |row| {
            Ok(WaterEntry {
                id: row.get(0)?,
                amount_ml: row.get::<_, i64>(1)? as u32,
                logged_at: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let total_ml: u32 = entries.iter().map(|e| e.amount_ml).sum();

    Ok(WaterDay {
        date,
        target_ml,
        total_ml,
        goal_reached: total_ml >= target_ml,
        entries,
    })
}

pub fn add(db: &Database, amount_ml: u32) -> AppResult<WaterDay> {
    if !(MIN_ENTRY_ML..=MAX_ENTRY_ML).contains(&amount_ml) {
        return Err(AppError::msg(format!(
            "Enter between {MIN_ENTRY_ML} ml and {MAX_ENTRY_ML} ml."
        )));
    }

    let now = now_ms();
    let date = crate::services::local_date(now);
    let conn = db.lock()?;
    conn.execute(
        "INSERT INTO water_intake_records (id, amount_ml, logged_at, local_date)
         VALUES (?1, ?2, ?3, ?4)",
        params![uuid::Uuid::new_v4().to_string(), amount_ml, now, date],
    )?;
    drop(conn);

    // The caller passes the current target; re-read it here would need the
    // settings service, so the target is filled in by the command layer.
    Ok(get_day(db, Some(date), 0)?)
}

pub fn remove(db: &Database, id: &str) -> AppResult<()> {
    let conn = db.lock()?;
    let affected = conn.execute(
        "DELETE FROM water_intake_records WHERE id = ?1",
        params![id],
    )?;
    if affected == 0 {
        return Err(AppError::msg("That record no longer exists."));
    }
    Ok(())
}

pub fn clear_day(db: &Database, date: Option<String>) -> AppResult<usize> {
    let date = date.unwrap_or_else(crate::services::today);
    let conn = db.lock()?;
    let removed = conn.execute(
        "DELETE FROM water_intake_records WHERE local_date = ?1",
        params![date],
    )?;
    Ok(removed)
}

/// The total for a date, without building the full day structure.
pub fn total_for_date(db: &Database, date: &str) -> AppResult<u32> {
    let conn = db.lock()?;
    let total: i64 = conn.query_row(
        "SELECT COALESCE(SUM(amount_ml), 0) FROM water_intake_records WHERE local_date = ?1",
        params![date],
        |row| row.get(0),
    )?;
    Ok(total as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AppSettings;

    fn db() -> Database {
        Database::open_in_memory().unwrap()
    }

    #[test]
    fn records_are_grouped_by_local_date() {
        let db = db();
        let today = crate::services::today();

        add(&db, 250).unwrap();
        add(&db, 500).unwrap();

        let day = get_day(&db, Some(today.clone()), 2000).unwrap();
        assert_eq!(day.total_ml, 750);
        assert_eq!(day.entries.len(), 2);
        assert!(!day.goal_reached);

        let other = get_day(&db, Some("1999-01-01".to_string()), 2000).unwrap();
        assert_eq!(other.total_ml, 0);
        assert!(other.entries.is_empty());
    }

    #[test]
    fn the_goal_is_reported_against_the_target() {
        let db = db();
        add(&db, 1000).unwrap();
        add(&db, 1000).unwrap();

        let day = get_day(&db, None, 2000).unwrap();
        assert_eq!(day.total_ml, 2000);
        assert!(day.goal_reached);

        let day = get_day(&db, None, 3000).unwrap();
        assert!(!day.goal_reached);
    }

    #[test]
    fn implausible_amounts_are_rejected() {
        let db = db();
        assert!(add(&db, 0).is_err());
        assert!(add(&db, 40_000).is_err());
    }

    #[test]
    fn removing_a_record_updates_the_total() {
        let db = db();
        add(&db, 250).unwrap();
        let day = get_day(&db, None, 2000).unwrap();
        let id = day.entries[0].id.clone();

        remove(&db, &id).unwrap();
        assert_eq!(get_day(&db, None, 2000).unwrap().total_ml, 0);
    }

    #[test]
    fn removing_a_missing_record_is_an_error_not_a_silent_success() {
        let db = db();
        assert!(remove(&db, "does-not-exist").is_err());
    }

    #[test]
    fn clearing_a_day_removes_only_that_day() {
        let db = db();
        add(&db, 250).unwrap();
        let conn = db.lock().unwrap();
        conn.execute(
            "INSERT INTO water_intake_records (id, amount_ml, logged_at, local_date)
             VALUES ('yesterday', 300, 0, '1999-01-01')",
            [],
        )
        .unwrap();
        drop(conn);

        assert_eq!(clear_day(&db, None).unwrap(), 1);
        assert_eq!(total_for_date(&db, "1999-01-01").unwrap(), 300);
    }

    #[test]
    fn the_default_target_is_two_litres() {
        let settings = AppSettings::default();
        assert_eq!(settings.water_target_ml, 2000);
    }
}
