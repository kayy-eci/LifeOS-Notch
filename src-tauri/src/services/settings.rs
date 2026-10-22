//! Settings storage.
//!
//! Settings live in `app_settings` as a single JSON document under the key
//! `app`. A patch is applied as a JSON merge, which keeps three properties
//! that a column-per-setting table would not have:
//!
//!   * adding a setting is a code change, not a migration;
//!   * `null` can clear a nullable field, which a plain `Option<T>` patch
//!     cannot express;
//!   * the whole document is written in one transaction, so a patch can never
//!     leave half the settings updated.

use rusqlite::params;

use crate::db::Database;
use crate::error::AppResult;
use crate::types::{now_ms, AppSettings};

const SETTINGS_KEY: &str = "app";

pub fn load(db: &Database) -> AppResult<AppSettings> {
    let conn = db.lock()?;
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            params![SETTINGS_KEY],
            |row| row.get(0),
        )
        .ok();

    let mut settings = match raw {
        Some(json) => serde_json::from_str::<AppSettings>(&json).unwrap_or_default(),
        None => AppSettings::default(),
    };
    settings = sanitise(settings);
    Ok(settings)
}

/// Applies a partial update and returns the merged, sanitised result.
pub fn update(db: &Database, patch: &serde_json::Value) -> AppResult<AppSettings> {
    let current = load(db)?;

    let mut merged = serde_json::to_value(&current)?;
    if let Some(object) = patch.as_object() {
        for (key, value) in object {
            // Unknown keys are ignored rather than rejected: an older build of
            // the frontend must not be able to break a newer backend.
            if merged.get(key).is_some() {
                merged[key] = value.clone();
            }
        }
    }

    let mut updated: AppSettings = serde_json::from_value(merged)?;
    updated = sanitise(updated);
    updated.updated_at = now_ms();

    let conn = db.lock()?;
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO app_settings (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![SETTINGS_KEY, serde_json::to_string(&updated)?, updated.updated_at],
    )?;
    tx.commit()?;

    Ok(updated)
}

/// Clamps every value into a sane range.
///
/// Settings arrive from a UI, so they are treated as untrusted input. A zero
/// focus duration or a 900 ml serving size would be silently accepted
/// otherwise, and the user would have no idea why the app behaved oddly.
pub fn sanitise(mut settings: AppSettings) -> AppSettings {
    settings.focus_minutes = settings.focus_minutes.clamp(1, 180);
    settings.short_break_minutes = settings.short_break_minutes.clamp(1, 60);
    settings.long_break_minutes = settings.long_break_minutes.clamp(1, 120);
    settings.long_break_after = settings.long_break_after.clamp(1, 12);

    settings.water_target_ml = settings.water_target_ml.clamp(250, 10_000);
    settings.water_serving_ml = settings.water_serving_ml.clamp(10, 2_000);
    settings.water_reminder_interval_minutes = settings.water_reminder_interval_minutes.clamp(5, 480);
    settings.water_reminder_start_hour = settings.water_reminder_start_hour.min(23);
    settings.water_reminder_end_hour = settings.water_reminder_end_hour.min(23);

    if settings.water_reminder_end_hour < settings.water_reminder_start_hour {
        // An inverted window would silently never fire; widen it instead.
        settings.water_reminder_end_hour = settings.water_reminder_start_hour + 1;
    }

    settings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sanitised() {
        let settings = sanitise(AppSettings::default());
        assert_eq!(settings.focus_minutes, 25);
        assert_eq!(settings.water_target_ml, 2000);
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let mut raw = AppSettings::default();
        raw.focus_minutes = 0;
        raw.water_serving_ml = 99_999;
        raw.water_reminder_start_hour = 30;
        raw.water_reminder_end_hour = 2;

        let settings = sanitise(raw);
        assert_eq!(settings.focus_minutes, 1);
        assert_eq!(settings.water_serving_ml, 2_000);
        assert_eq!(settings.water_reminder_start_hour, 23);
        assert!(settings.water_reminder_end_hour >= settings.water_reminder_start_hour);
    }

    #[test]
    fn update_merges_and_persists() {
        let db = Database::open_in_memory().unwrap();
        let patch = serde_json::json!({ "focus_minutes": 50, "notifications_enabled": false });

        let updated = update(&db, &patch).unwrap();
        assert_eq!(updated.focus_minutes, 50);
        assert!(!updated.notifications_enabled);
        // Untouched values survive the patch.
        assert_eq!(updated.water_target_ml, 2000);

        let reloaded = load(&db).unwrap();
        assert_eq!(reloaded.focus_minutes, 50);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let db = Database::open_in_memory().unwrap();
        let patch = serde_json::json!({ "not_a_real_setting": 123, "focus_minutes": 30 });
        let updated = update(&db, &patch).unwrap();
        assert_eq!(updated.focus_minutes, 30);
    }

    #[test]
    fn null_clears_a_nullable_field() {
        let db = Database::open_in_memory().unwrap();
        update(&db, &serde_json::json!({ "notion_database_id": "abc" })).unwrap();
        let cleared = update(&db, &serde_json::json!({ "notion_database_id": null })).unwrap();
        assert!(cleared.notion_database_id.is_none());
    }
}
