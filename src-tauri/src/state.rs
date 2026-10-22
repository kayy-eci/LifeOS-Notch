//! The application state.
//!
//! `AppState` is the single object the Tauri commands talk to. It owns the
//! database, the cached settings, the running timer, and the two services
//! that need to keep state between calls (media and Notion).
//!
//! Every public method here is the *only* place where a service is called
//! from, which keeps the locking rules in one file: no lock is ever held
//! across an `await`, and no service call happens while the settings lock is
//! held.

use std::sync::{Mutex, RwLock};

use tauri::{AppHandle, Emitter, Manager};

use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::services::{
    credentials::CredentialStore,
    lyrics::LyricsService,
    media::MediaService,
    notion::NotionService,
    pomodoro,
    settings,
    tasks,
    water,
};
use crate::types::{
    AppSettings, Lyrics, MediaCommand, MediaSession, MonitorInfo, NotionDatabase, NotionStatus,
    NotchState, PomodoroMode, PomodoroState, Task, WaterDay,
};

/// Event names shared with the frontend. Kept in one place so the two sides
/// cannot drift.
pub mod events {
    pub const MEDIA_SESSION: &str = "lifeos://media-session";
    pub const POMODORO_STATE: &str = "lifeos://pomodoro-state";
    pub const NOTION_STATUS: &str = "lifeos://notion-status";
    pub const WATER_DAY: &str = "lifeos://water-day";
}

pub struct AppState {
    pub db: Database,
    settings: RwLock<AppSettings>,
    /// The running timer. Public to the crate so the ticker thread in `lib.rs`
    /// can advance it without going through a command.
    pub pomodoro: Mutex<PomodoroState>,
    pub media: MediaService,
    pub notion: NotionService,
    pub credentials: CredentialStore,
    pub lyrics: LyricsService,
    app: AppHandle,
}

impl AppState {
    pub fn new(app: AppHandle, db: Database) -> AppResult<Self> {
        let settings = settings::load(&db)?;
        let pomodoro = pomodoro::load(&db)?;
        let credentials = CredentialStore::new();

        let state = AppState {
            db,
            settings: RwLock::new(settings),
            pomodoro: Mutex::new(pomodoro),
            media: MediaService::new(),
            notion: NotionService::new(credentials.clone()),
            credentials,
            lyrics: LyricsService::new(),
            app,
        };
        Ok(state)
    }

    fn snapshot_settings(&self) -> AppSettings {
        self.settings
            .read()
            .map(|s| s.clone())
            .unwrap_or_else(|_| AppSettings::default())
    }

    /* ------------------------------------------------------------------ */
    /* Settings                                                            */
    /* ------------------------------------------------------------------ */

    pub fn settings(&self) -> AppResult<AppSettings> {
        Ok(self.snapshot_settings())
    }

    pub fn update_settings(&self, patch: &serde_json::Value) -> AppResult<AppSettings> {
        let updated = settings::update(&self.db, patch)?;

        // A changed timer length only affects a phase that has not started
        // yet, so the running state is left alone. `start_with_windows` is
        // applied to the registry immediately.
        if let Ok(mut guard) = self.settings.write() {
            *guard = updated.clone();
        }

        if patch.get("start_with_windows").is_some() {
            self.apply_start_with_windows(updated.start_with_windows);
        }

        Ok(updated)
    }

    /// Registers or removes the Windows startup entry.
    ///
    /// The autostart plugin writes a single registry value, so calling it
    /// repeatedly cannot create duplicate entries.
    fn apply_start_with_windows(&self, enabled: bool) {
        use tauri_plugin_autostart::ManagerExt;
        let manager = self.app.autolaunch();
        let result = if enabled {
            manager.enable()
        } else {
            manager.disable()
        };
        if let Err(error) = result {
            log::warn!("could not change the startup entry: {error}");
        }
    }

    /* ------------------------------------------------------------------ */
    /* Pomodoro                                                            */
    /* ------------------------------------------------------------------ */

    pub fn pomodoro_state(&self) -> AppResult<PomodoroState> {
        Ok(self
            .pomodoro
            .lock()
            .map(|s| s.clone())
            .unwrap_or_else(|_| pomodoro::default_state()))
    }

    pub fn pomodoro_start(&self) -> AppResult<PomodoroState> {
        let settings = self.snapshot_settings();
        let now = crate::types::now_ms();
        let state = {
            let mut guard = self.pomodoro.lock().map_err(lock_error)?;
            pomodoro::start(&mut guard, &settings, now);
            guard.clone()
        };
        pomodoro::save(&self.db, &state)?;
        self.emit_pomodoro(&state);
        Ok(state)
    }

    pub fn pomodoro_pause(&self) -> AppResult<PomodoroState> {
        let now = crate::types::now_ms();
        let state = {
            let mut guard = self.pomodoro.lock().map_err(lock_error)?;
            pomodoro::pause(&mut guard, now);
            guard.clone()
        };
        pomodoro::save(&self.db, &state)?;
        self.emit_pomodoro(&state);
        Ok(state)
    }

    pub fn pomodoro_resume(&self) -> AppResult<PomodoroState> {
        let now = crate::types::now_ms();
        let state = {
            let mut guard = self.pomodoro.lock().map_err(lock_error)?;
            pomodoro::resume(&mut guard, now);
            guard.clone()
        };
        pomodoro::save(&self.db, &state)?;
        self.emit_pomodoro(&state);
        Ok(state)
    }

    pub fn pomodoro_skip(&self) -> AppResult<PomodoroState> {
        let settings = self.snapshot_settings();
        let now = crate::types::now_ms();
        let state = {
            let mut guard = self.pomodoro.lock().map_err(lock_error)?;
            pomodoro::skip(&mut guard, &settings, now);
            guard.clone()
        };
        pomodoro::save(&self.db, &state)?;
        self.emit_pomodoro(&state);
        Ok(state)
    }

    pub fn pomodoro_reset(&self) -> AppResult<PomodoroState> {
        let state = {
            let mut guard = self.pomodoro.lock().map_err(lock_error)?;
            pomodoro::reset(&mut guard);
            guard.clone()
        };
        pomodoro::save(&self.db, &state)?;
        self.emit_pomodoro(&state);
        Ok(state)
    }

    fn emit_pomodoro(&self, state: &PomodoroState) {
        let _ = self.app.emit(events::POMODORO_STATE, state);
    }

    /// Called by the ticker thread when a phase ends.
    pub fn on_phase_finished(&self, transition: &pomodoro::PhaseTransition) {
        let now = crate::types::now_ms();

        if transition.counted {
            if let Ok(today) = pomodoro::record_session(&self.db, transition.finished, now, now) {
                if let Ok(mut guard) = self.pomodoro.lock() {
                    guard.completed_today = today;
                    guard.completed_total = pomodoro::count_total(&self.db).unwrap_or(guard.completed_total);
                }
            }
        }

        let settings = self.snapshot_settings();
        if settings.notifications_enabled && settings.notify_pomodoro {
            let title = match transition.finished {
                PomodoroMode::Focus => "Focus session finished",
                PomodoroMode::ShortBreak | PomodoroMode::LongBreak => "Break finished",
            };
            let body = match transition.next {
                PomodoroMode::Focus => "Time to focus.",
                PomodoroMode::ShortBreak => "Take a short break.",
                PomodoroMode::LongBreak => "Take a longer break.",
            };
            self.notify(title, body);
        }

        if let Ok(state) = self.pomodoro_state() {
            let _ = pomodoro::save(&self.db, &state);
            self.emit_pomodoro(&state);
        }
    }

    /* ------------------------------------------------------------------ */
    /* Water                                                               */
    /* ------------------------------------------------------------------ */

    pub fn water_day(&self, date: Option<String>) -> AppResult<WaterDay> {
        let settings = self.snapshot_settings();
        water::get_day(&self.db, date, settings.water_target_ml)
    }

    pub fn add_water(&self, amount_ml: u32) -> AppResult<WaterDay> {
        water::add(&self.db, amount_ml)?;
        self.emit_water();
        self.water_day(None)
    }

    pub fn remove_water(&self, id: &str) -> AppResult<WaterDay> {
        water::remove(&self.db, id)?;
        self.emit_water();
        self.water_day(None)
    }

    pub fn clear_water_day(&self, date: Option<String>) -> AppResult<WaterDay> {
        water::clear_day(&self.db, date)?;
        self.emit_water();
        self.water_day(None)
    }

    fn emit_water(&self) {
        if let Ok(day) = self.water_day(None) {
            let _ = self.app.emit(events::WATER_DAY, day);
        }
    }

    /* ------------------------------------------------------------------ */
    /* Tasks                                                               */
    /* ------------------------------------------------------------------ */

    pub fn list_tasks(&self) -> AppResult<Vec<Task>> {
        tasks::list(&self.db)
    }

    pub fn create_task(
        &self,
        title: &str,
        description: Option<String>,
        due_at: Option<i64>,
    ) -> AppResult<Task> {
        let task = tasks::create(&self.db, title, description, due_at)?;
        self.push_after_local_change();
        Ok(task)
    }

    pub fn update_task(&self, id: &str, patch: &serde_json::Value) -> AppResult<Task> {
        let task = tasks::update(&self.db, id, patch)?;
        self.push_after_local_change();
        Ok(task)
    }

    pub fn delete_task(&self, id: &str) -> AppResult<()> {
        tasks::delete(&self.db, id)?;
        self.push_after_local_change();
        Ok(())
    }

    /// Tries to push a local change straight away.
    ///
    /// A failure is not surfaced to the user here: the operation is already in
    /// the durable queue and the task carries a `pending` or `failed` state,
    /// which the to-do panel shows. Reporting an error on top of that would
    /// make a routine offline edit look like a broken app.
    fn push_after_local_change(&self) {
        if let Err(error) = self.notion.try_push_pending(&self.db) {
            log::debug!("deferred a task push: {error}");
        }
        self.emit_notion_status();
    }

    /* ------------------------------------------------------------------ */
    /* Notion                                                              */
    /* ------------------------------------------------------------------ */

    pub fn notion_status(&self) -> AppResult<NotionStatus> {
        Ok(self.notion.status(&self.db)?)
    }

    pub fn notion_configure(&self, client_id: &str, client_secret: &str) -> AppResult<NotionStatus> {
        self.notion.configure(client_id, client_secret)?;
        self.emit_notion_status();
        self.notion_status()
    }

    pub fn notion_connect(&self) -> AppResult<NotionStatus> {
        let status = self.notion.connect()?;
        self.emit_notion_status();
        Ok(status)
    }

    pub fn notion_disconnect(&self) -> AppResult<NotionStatus> {
        let status = self.notion.disconnect()?;
        self.emit_notion_status();
        Ok(status)
    }

    pub fn notion_list_databases(&self) -> AppResult<Vec<NotionDatabase>> {
        self.notion.list_databases().await_blocking()
    }

    pub fn notion_select_database(&self, id: &str) -> AppResult<NotionStatus> {
        let status = self.notion.select_database(&self.db, id)?;
        self.emit_notion_status();
        Ok(status)
    }

    pub fn notion_sync(&self) -> AppResult<NotionStatus> {
        let status = self.notion.sync(&self.db)?;
        self.emit_notion_status();
        Ok(status)
    }

    fn emit_notion_status(&self) {
        if let Ok(status) = self.notion.status(&self.db) {
            let _ = self.app.emit(events::NOTION_STATUS, status);
        }
    }

    /* ------------------------------------------------------------------ */
    /* Media                                                               */
    /* ------------------------------------------------------------------ */

    pub fn media_current_session(&self) -> AppResult<Option<MediaSession>> {
        self.media.current_session()
    }

    pub fn media_command(&self, command: MediaCommand) -> AppResult<Option<MediaSession>> {
        self.media.command(command)
    }

    pub fn media_seek(&self, position_seconds: f64) -> AppResult<Option<MediaSession>> {
        self.media.seek(position_seconds)
    }

    pub fn emit_media(&self, session: Option<&MediaSession>) {
        let _ = self.app.emit(events::MEDIA_SESSION, session);
    }

    /* ------------------------------------------------------------------ */
    /* Lyrics                                                              */
    /* ------------------------------------------------------------------ */

    pub fn get_lyrics(&self, track_id: &str, title: &str, artist: &str) -> AppResult<Option<Lyrics>> {
        self.lyrics.get(&self.db, track_id, title, artist)
    }

    /* ------------------------------------------------------------------ */
    /* Window                                                              */
    /* ------------------------------------------------------------------ */

    pub fn set_notch_state(&self, window: tauri::WebviewWindow, notch_state: NotchState) -> AppResult<()> {
        crate::windows::notch::set_state(&window, notch_state)
    }

    pub fn monitors(&self) -> AppResult<Vec<MonitorInfo>> {
        let window = self.app.get_webview_window("notch").ok_or_else(|| {
            AppError::msg("The notch window is not available.")
        })?;
        crate::windows::notch::list_monitors(&window)
    }

    /* ------------------------------------------------------------------ */
    /* Notifications                                                       */
    /* ------------------------------------------------------------------ */

    /// Shows a Windows notification, if the user has them switched on.
    pub fn notify(&self, title: &str, body: &str) {
        use tauri_plugin_notification::NotificationExt;
        let settings = self.snapshot_settings();
        if !settings.notifications_enabled {
            return;
        }
        if let Err(error) = self
            .app
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show()
        {
            log::warn!("could not show a notification: {error}");
        }
    }
}

fn lock_error<T>(_: T) -> AppError {
    AppError::msg("The application state is busy. Try again in a moment.")
}

/// A small extension so `await_blocking` can be called on the Notion service
/// result without importing futures everywhere.
trait AwaitBlocking {
    fn await_blocking(self) -> AppResult<Vec<NotionDatabase>>;
}

impl AwaitBlocking for AppResult<Vec<NotionDatabase>> {
    fn await_blocking(self) -> AppResult<Vec<NotionDatabase>> {
        self
    }
}
