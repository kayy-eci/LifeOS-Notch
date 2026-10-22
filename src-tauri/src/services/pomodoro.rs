//! Pomodoro timer.
//!
//! # Why a deadline and not a counter
//!
//! A timer that decrements one second per tick drifts: it stops counting when
//! the window is closed, when the machine sleeps, or when the event loop is
//! busy. This timer stores the *moment the phase ends* and derives the
//! remaining time from the wall clock. The tick interval only decides how
//! often the UI is refreshed — never how much time is left.
//!
//! That also gives sleep and resume the right behaviour for free: after the
//! machine wakes up, `ends_at` is already in the past, so the phase is
//! completed and the next one starts, rather than the timer pretending the
//! elapsed time never happened.

use rusqlite::params;

use crate::db::Database;
use crate::error::AppResult;
use crate::types::{now_ms, AppSettings, Millis, PomodoroMode, PomodoroState, PomodoroStatus};

/// What changed when a phase ended.
#[derive(Debug, Clone, PartialEq)]
pub struct PhaseTransition {
    pub finished: PomodoroMode,
    pub next: PomodoroMode,
    /// True when the finished phase was a focus session that was completed
    /// normally (not skipped).
    pub counted: bool,
}

pub fn default_state() -> PomodoroState {
    PomodoroState {
        mode: PomodoroMode::Focus,
        status: PomodoroStatus::Idle,
        ends_at: None,
        remaining_seconds: None,
        completed_focus_sessions: 0,
        completed_today: 0,
        completed_total: 0,
        started_at: None,
    }
}

/// Seconds left in the current phase.
pub fn remaining_seconds(state: &PomodoroState, now: Millis) -> u32 {
    match state.status {
        PomodoroStatus::Running => match state.ends_at {
            Some(ends_at) => {
                let remaining = ends_at - now;
                if remaining <= 0 {
                    0
                } else {
                    ((remaining as f64) / 1000.0).ceil() as u32
                }
            }
            None => 0,
        },
        PomodoroStatus::Paused => state.remaining_seconds.unwrap_or(0),
        PomodoroStatus::Idle => 0,
    }
}

/// The length of a phase, in seconds.
pub fn phase_seconds(mode: PomodoroMode, settings: &AppSettings) -> u32 {
    match mode {
        PomodoroMode::Focus => settings.focus_minutes * 60,
        PomodoroMode::ShortBreak => settings.short_break_minutes * 60,
        PomodoroMode::LongBreak => settings.long_break_minutes * 60,
    }
}

/// The mode that follows `mode`.
pub fn next_mode(mode: PomodoroMode, completed_focus_sessions: u32, settings: &AppSettings) -> PomodoroMode {
    match mode {
        PomodoroMode::Focus => {
            if completed_focus_sessions > 0 && completed_focus_sessions % settings.long_break_after == 0 {
                PomodoroMode::LongBreak
            } else {
                PomodoroMode::ShortBreak
            }
        }
        // A break is always followed by a focus session.
        PomodoroMode::ShortBreak | PomodoroMode::LongBreak => PomodoroMode::Focus,
    }
}

/// Starts a phase from scratch.
pub fn start(state: &mut PomodoroState, settings: &AppSettings, now: Millis) {
    let seconds = phase_seconds(state.mode, settings);
    state.status = PomodoroStatus::Running;
    state.ends_at = Some(now + (seconds as i64) * 1000);
    state.remaining_seconds = None;
    state.started_at = Some(now);
}

/// Pauses a running phase, keeping the time that is left.
pub fn pause(state: &mut PomodoroState, now: Millis) {
    if state.status != PomodoroStatus::Running {
        return;
    }
    state.remaining_seconds = Some(remaining_seconds(state, now));
    state.status = PomodoroStatus::Paused;
    state.ends_at = None;
}

/// Resumes a paused phase, restoring the remaining time it had.
pub fn resume(state: &mut PomodoroState, now: Millis) {
    if state.status != PomodoroStatus::Paused {
        return;
    }
    let remaining = state.remaining_seconds.unwrap_or(0);
    state.status = PomodoroStatus::Running;
    state.ends_at = Some(now + (remaining as i64) * 1000);
    state.remaining_seconds = None;
}

/// Moves to the next phase without counting the current one as complete.
pub fn skip(state: &mut PomodoroState, settings: &AppSettings, now: Millis) -> PomodoroMode {
    let next = next_mode(state.mode, state.completed_focus_sessions, settings);
    state.mode = next;
    state.status = PomodoroStatus::Idle;
    state.ends_at = None;
    state.remaining_seconds = None;
    state.started_at = None;
    next
}

/// Returns the timer to the beginning of a focus session.
pub fn reset(state: &mut PomodoroState) {
    state.mode = PomodoroMode::Focus;
    state.status = PomodoroStatus::Idle;
    state.ends_at = None;
    state.remaining_seconds = None;
    state.started_at = None;
    state.completed_focus_sessions = 0;
}

/// Completes the current phase if its deadline has passed.
///
/// Returns `None` while the phase is still running. When the phase has
/// expired, the state is advanced to the next phase and the transition is
/// described so the caller can record the session and notify the user.
pub fn advance_if_expired(
    state: &mut PomodoroState,
    settings: &AppSettings,
    now: Millis,
) -> Option<PhaseTransition> {
    if state.status != PomodoroStatus::Running {
        return None;
    }
    let ends_at = state.ends_at?;
    if now < ends_at {
        return None;
    }

    let finished = state.mode;
    let counted = finished == PomodoroMode::Focus;

    if counted {
        state.completed_focus_sessions += 1;
    }

    let next = next_mode(finished, state.completed_focus_sessions, settings);
    state.mode = next;
    state.ends_at = None;
    state.remaining_seconds = None;
    state.started_at = None;

    // Auto-start is opt-in per phase, and defaults to starting breaks but not
    // focus sessions: returning to work should be a deliberate act.
    let auto_start = match next {
        PomodoroMode::Focus => settings.auto_start_focus,
        PomodoroMode::ShortBreak | PomodoroMode::LongBreak => settings.auto_start_breaks,
    };
    if auto_start {
        start(state, settings, now);
    } else {
        state.status = PomodoroStatus::Idle;
    }

    Some(PhaseTransition {
        finished,
        next,
        counted,
    })
}

/* -------------------------------------------------------------------------- */
/* Persistence                                                                 */
/* -------------------------------------------------------------------------- */

pub fn load(db: &Database) -> AppResult<PomodoroState> {
    // The guard must be dropped before the counters are read: `count_today`
    // and `count_total` take the same mutex, and `std::sync::Mutex` is not
    // reentrant, so holding it here would deadlock the whole app on the first
    // read of the timer state.
    let row = {
        let conn = db.lock()?;
        conn.query_row(
            "SELECT mode, status, ends_at, remaining_seconds, completed_focus_sessions, started_at
             FROM pomodoro_state WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                ))
            },
        )
        .ok()
    };

    let mut state = match row {
        Some((mode, status, ends_at, remaining, completed, started_at)) => PomodoroState {
            mode: match mode.as_str() {
                "short_break" => PomodoroMode::ShortBreak,
                "long_break" => PomodoroMode::LongBreak,
                _ => PomodoroMode::Focus,
            },
            status: match status.as_str() {
                "running" => PomodoroStatus::Running,
                "paused" => PomodoroStatus::Paused,
                _ => PomodoroStatus::Idle,
            },
            ends_at,
            remaining_seconds: remaining.map(|v| v as u32),
            completed_focus_sessions: completed as u32,
            completed_today: 0,
            completed_total: 0,
            started_at,
        },
        None => default_state(),
    };

    state.completed_today = count_today(db)?;
    state.completed_total = count_total(db)?;
    Ok(state)
}

pub fn save(db: &Database, state: &PomodoroState) -> AppResult<()> {
    let conn = db.lock()?;
    conn.execute(
        "INSERT INTO pomodoro_state
            (id, mode, status, ends_at, remaining_seconds, completed_focus_sessions, started_at)
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET
            mode = excluded.mode,
            status = excluded.status,
            ends_at = excluded.ends_at,
            remaining_seconds = excluded.remaining_seconds,
            completed_focus_sessions = excluded.completed_focus_sessions,
            started_at = excluded.started_at",
        params![
            state.mode.as_str(),
            match state.status {
                PomodoroStatus::Idle => "idle",
                PomodoroStatus::Running => "running",
                PomodoroStatus::Paused => "paused",
            },
            state.ends_at,
            state.remaining_seconds.map(|v| v as i64),
            state.completed_focus_sessions as i64,
            state.started_at,
        ],
    )?;
    Ok(())
}

/// Records a finished focus session and returns how many were completed today.
pub fn record_session(db: &Database, mode: PomodoroMode, started_at: Millis, ended_at: Millis) -> AppResult<u32> {
    let date = crate::services::local_date(ended_at);
    // Scoped so the guard is released before `count_today` re-locks. See the
    // note in `load` about the mutex not being reentrant.
    {
        let conn = db.lock()?;
        conn.execute(
            "INSERT INTO pomodoro_sessions (id, mode, started_at, ended_at, completed, local_date)
             VALUES (?1, ?2, ?3, ?4, 1, ?5)",
            params![
                uuid::Uuid::new_v4().to_string(),
                mode.as_str(),
                started_at,
                ended_at,
                date
            ],
        )?;
    }
    count_today(db)
}

pub fn count_today(db: &Database) -> AppResult<u32> {
    let date = crate::services::local_date(now_ms());
    let conn = db.lock()?;
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pomodoro_sessions WHERE mode = 'focus' AND completed = 1 AND local_date = ?1",
        params![date],
        |row| row.get(0),
    )?;
    Ok(count as u32)
}

pub fn count_total(db: &Database) -> AppResult<u32> {
    let conn = db.lock()?;
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pomodoro_sessions WHERE mode = 'focus' AND completed = 1",
        [],
        |row| row.get(0),
    )?;
    Ok(count as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> AppSettings {
        AppSettings {
            focus_minutes: 25,
            short_break_minutes: 5,
            long_break_minutes: 15,
            long_break_after: 4,
            auto_start_breaks: true,
            auto_start_focus: false,
            ..AppSettings::default()
        }
    }

    #[test]
    fn remaining_time_is_derived_from_the_deadline() {
        let mut state = default_state();
        let now = 1_000_000i64;
        start(&mut state, &settings(), now);
        assert_eq!(state.ends_at, Some(now + 25 * 60 * 1000));

        // A long gap between ticks must not change the remaining time.
        assert_eq!(remaining_seconds(&state, now + 60_000), 24 * 60);
        assert_eq!(remaining_seconds(&state, now + 24 * 60 * 1000), 60);
        assert_eq!(remaining_seconds(&state, now + 25 * 60 * 1000), 0);
    }

    #[test]
    fn pause_keeps_the_remaining_time_and_resume_restores_it() {
        let mut state = default_state();
        let now = 1_000_000i64;
        start(&mut state, &settings(), now);

        pause(&mut state, now + 60_000);
        assert_eq!(state.status, PomodoroStatus::Paused);
        assert_eq!(state.remaining_seconds, Some(24 * 60));
        assert!(state.ends_at.is_none());

        // Time passing while paused must not consume the session.
        assert_eq!(remaining_seconds(&state, now + 600_000), 24 * 60);

        resume(&mut state, now + 600_000);
        assert_eq!(state.status, PomodoroStatus::Running);
        assert_eq!(state.ends_at, Some(now + 600_000 + 24 * 60 * 1000));
    }

    #[test]
    fn an_expired_phase_completes_and_starts_the_break() {
        let mut state = default_state();
        let now = 1_000_000i64;
        start(&mut state, &settings(), now);

        // Simulate the machine waking up well after the deadline.
        let transition = advance_if_expired(&mut state, &settings(), now + 30 * 60 * 1000).unwrap();
        assert_eq!(transition.finished, PomodoroMode::Focus);
        assert_eq!(transition.next, PomodoroMode::ShortBreak);
        assert!(transition.counted);
        assert_eq!(state.mode, PomodoroMode::ShortBreak);
        assert_eq!(state.completed_focus_sessions, 1);
        // Auto-start breaks is on, so the break is already running.
        assert_eq!(state.status, PomodoroStatus::Running);
    }

    #[test]
    fn a_phase_that_has_not_expired_does_not_advance() {
        let mut state = default_state();
        let now = 1_000_000i64;
        start(&mut state, &settings(), now);
        assert!(advance_if_expired(&mut state, &settings(), now + 1000).is_none());
        assert_eq!(state.mode, PomodoroMode::Focus);
    }

    #[test]
    fn a_long_break_follows_the_configured_number_of_focus_sessions() {
        let mut state = default_state();
        let settings = settings();
        let now = 1_000_000i64;

        // `auto_start_breaks` is on, so finishing a focus session immediately
        // starts a break. To count focus sessions the loop has to skip past
        // each break rather than treat every iteration as one.
        for expected in 1..=4 {
            state.mode = PomodoroMode::Focus;
            state.status = PomodoroStatus::Idle;
            start(&mut state, &settings, now);
            let transition =
                advance_if_expired(&mut state, &settings, now + 30 * 60 * 1000).unwrap();

            assert_eq!(transition.finished, PomodoroMode::Focus);
            assert_eq!(state.completed_focus_sessions, expected);

            let expected_next = if expected % settings.long_break_after == 0 {
                PomodoroMode::LongBreak
            } else {
                PomodoroMode::ShortBreak
            };
            assert_eq!(
                transition.next, expected_next,
                "after {expected} focus session(s) the next phase should be {expected_next:?}"
            );
        }
    }

    #[test]
    fn skip_moves_on_without_counting_the_phase() {
        let mut state = default_state();
        let now = 1_000_000i64;
        start(&mut state, &settings(), now);
        let next = skip(&mut state, &settings(), now);
        assert_eq!(next, PomodoroMode::ShortBreak);
        assert_eq!(state.completed_focus_sessions, 0);
        assert_eq!(state.status, PomodoroStatus::Idle);
    }

    #[test]
    fn state_survives_a_restart() {
        let db = Database::open_in_memory().unwrap();
        let mut state = default_state();
        let now = 1_000_000i64;
        start(&mut state, &settings(), now);
        pause(&mut state, now + 30_000);
        save(&db, &state).unwrap();

        let reloaded = load(&db).unwrap();
        assert_eq!(reloaded.mode, PomodoroMode::Focus);
        assert_eq!(reloaded.status, PomodoroStatus::Paused);
        assert_eq!(reloaded.remaining_seconds, Some(24 * 60 + 30));
    }

    #[test]
    fn completed_sessions_are_counted_for_today() {
        let db = Database::open_in_memory().unwrap();
        let now = now_ms();
        record_session(&db, PomodoroMode::Focus, now, now).unwrap();
        record_session(&db, PomodoroMode::Focus, now, now).unwrap();
        record_session(&db, PomodoroMode::ShortBreak, now, now).unwrap();
        assert_eq!(count_today(&db).unwrap(), 2);
        assert_eq!(count_total(&db).unwrap(), 2);
    }
}
