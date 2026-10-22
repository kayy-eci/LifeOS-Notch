//! Backend services.
//!
//! Each service owns one area of behaviour and exposes plain functions that
//! take the database and return data. Commands are thin wrappers over these.

pub mod credentials;
pub mod lyrics;
pub mod media;
pub mod notion;
pub mod pomodoro;
pub mod settings;
pub mod tasks;
pub mod water;

use chrono::{DateTime, Local, TimeZone, Timelike, Utc};

/// The local calendar date for a timestamp, as `YYYY-MM-DD`.
///
/// Water totals and daily reminder windows are defined against the user's
/// local day, not UTC: a glass of water at 23:30 in Jakarta belongs to that
/// day, whatever the clock in London says.
///
/// The local date is *stored* on each record at the moment it is written. If
/// the user later changes their time zone, past records keep the day they
/// were recorded on. Re-deriving them would silently move yesterday's
/// intake into today, which is worse than a record that reflects the day the
/// user actually drank the water.
pub fn local_date(timestamp_ms: i64) -> String {
    let utc: DateTime<Utc> = Utc
        .timestamp_millis_opt(timestamp_ms)
        .single()
        .unwrap_or_else(Utc::now);
    let local: DateTime<Local> = utc.with_timezone(&Local);
    local.format("%Y-%m-%d").to_string()
}

/// Today's local date.
pub fn today() -> String {
    local_date(crate::types::now_ms())
}

/// The current local hour, 0..23.
pub fn current_hour() -> u32 {
    Local::now().hour()
}
