//! Tauri commands: the only way the frontend can reach the backend.
//!
//! Commands are thin. They validate their input, call a service, and return
//! the service's result. No business logic lives here — it lives in
//! `services/`, where it can be unit tested without a Tauri runtime.

use serde::Serialize;

use crate::error::AppError;
use crate::state::AppState;
use crate::types::{
    AppSettings, Lyrics, MediaCommand, MediaSession, MonitorInfo, NotionDatabase, NotionStatus,
    PomodoroState, Task, WaterDay,
};

/// Serialises a command result so the frontend always receives the same
/// envelope: `Ok` carries data, `Err` carries a message the user can read.
type CommandResult<T> = Result<T, AppError>;

/* -------------------------------------------------------------------------- */
/* Settings                                                                    */
/* -------------------------------------------------------------------------- */

#[tauri::command]
pub fn get_settings(state: tauri::State<'_, std::sync::Arc<AppState>>) -> CommandResult<AppSettings> {
    state.settings()
}

#[tauri::command]
pub fn update_settings(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    patch: serde_json::Value,
) -> CommandResult<AppSettings> {
    state.update_settings(&patch)
}

/* -------------------------------------------------------------------------- */
/* Pomodoro                                                                    */
/* -------------------------------------------------------------------------- */

#[tauri::command]
pub fn get_pomodoro_state(state: tauri::State<'_, std::sync::Arc<AppState>>) -> CommandResult<PomodoroState> {
    state.pomodoro_state()
}

#[tauri::command]
pub fn pomodoro_start(state: tauri::State<'_, std::sync::Arc<AppState>>) -> CommandResult<PomodoroState> {
    state.pomodoro_start()
}

#[tauri::command]
pub fn pomodoro_pause(state: tauri::State<'_, std::sync::Arc<AppState>>) -> CommandResult<PomodoroState> {
    state.pomodoro_pause()
}

#[tauri::command]
pub fn pomodoro_resume(state: tauri::State<'_, std::sync::Arc<AppState>>) -> CommandResult<PomodoroState> {
    state.pomodoro_resume()
}

#[tauri::command]
pub fn pomodoro_skip(state: tauri::State<'_, std::sync::Arc<AppState>>) -> CommandResult<PomodoroState> {
    state.pomodoro_skip()
}

#[tauri::command]
pub fn pomodoro_reset(state: tauri::State<'_, std::sync::Arc<AppState>>) -> CommandResult<PomodoroState> {
    state.pomodoro_reset()
}

/* -------------------------------------------------------------------------- */
/* Water                                                                       */
/* -------------------------------------------------------------------------- */

#[tauri::command]
pub fn get_water_day(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    date: Option<String>,
) -> CommandResult<WaterDay> {
    state.water_day(date)
}

#[tauri::command]
pub fn add_water(state: tauri::State<'_, std::sync::Arc<AppState>>, amount_ml: u32) -> CommandResult<WaterDay> {
    state.add_water(amount_ml)
}

#[tauri::command]
pub fn remove_water(state: tauri::State<'_, std::sync::Arc<AppState>>, id: String) -> CommandResult<WaterDay> {
    state.remove_water(&id)
}

#[tauri::command]
pub fn clear_water_day(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    date: Option<String>,
) -> CommandResult<WaterDay> {
    state.clear_water_day(date)
}

/* -------------------------------------------------------------------------- */
/* Tasks                                                                       */
/* -------------------------------------------------------------------------- */

#[tauri::command]
pub fn list_tasks(state: tauri::State<'_, std::sync::Arc<AppState>>) -> CommandResult<Vec<Task>> {
    state.list_tasks()
}

#[derive(serde::Deserialize)]
pub struct CreateTaskInput {
    pub title: String,
    #[serde(default)]
    pub due_at: Option<i64>,
    #[serde(default)]
    pub description: Option<String>,
}

#[tauri::command]
pub fn create_task(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    input: CreateTaskInput,
) -> CommandResult<Task> {
    state.create_task(&input.title, input.description, input.due_at)
}

#[tauri::command]
pub fn update_task(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    id: String,
    patch: serde_json::Value,
) -> CommandResult<Task> {
    state.update_task(&id, &patch)
}

#[tauri::command]
pub fn delete_task(state: tauri::State<'_, std::sync::Arc<AppState>>, id: String) -> CommandResult<()> {
    state.delete_task(&id)
}

/* -------------------------------------------------------------------------- */
/* Notion                                                                      */
/* -------------------------------------------------------------------------- */

#[tauri::command]
pub fn notion_status(state: tauri::State<'_, std::sync::Arc<AppState>>) -> CommandResult<NotionStatus> {
    state.notion_status()
}

#[tauri::command]
pub fn notion_configure(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    client_id: String,
    client_secret: String,
) -> CommandResult<NotionStatus> {
    state.notion_configure(&client_id, &client_secret)
}

#[tauri::command]
pub fn notion_connect(state: tauri::State<'_, std::sync::Arc<AppState>>) -> CommandResult<NotionStatus> {
    state.notion_connect()
}

#[tauri::command]
pub fn notion_disconnect(state: tauri::State<'_, std::sync::Arc<AppState>>) -> CommandResult<NotionStatus> {
    state.notion_disconnect()
}

#[tauri::command]
pub fn notion_list_databases(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> CommandResult<Vec<NotionDatabase>> {
    state.notion_list_databases()
}

#[tauri::command]
pub fn notion_select_database(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    id: String,
) -> CommandResult<NotionStatus> {
    state.notion_select_database(&id)
}

#[tauri::command]
pub fn notion_sync(state: tauri::State<'_, std::sync::Arc<AppState>>) -> CommandResult<NotionStatus> {
    state.notion_sync()
}

/* -------------------------------------------------------------------------- */
/* Media                                                                       */
/* -------------------------------------------------------------------------- */

#[tauri::command]
pub fn media_current_session(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> CommandResult<Option<MediaSession>> {
    state.media_current_session()
}

#[tauri::command]
pub fn media_command(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    command: MediaCommand,
) -> CommandResult<Option<MediaSession>> {
    state.media_command(command)
}

#[tauri::command]
pub fn media_seek(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    position_seconds: f64,
) -> CommandResult<Option<MediaSession>> {
    state.media_seek(position_seconds)
}

/* -------------------------------------------------------------------------- */
/* Lyrics                                                                      */
/* -------------------------------------------------------------------------- */

#[tauri::command]
pub fn get_lyrics(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    track_id: String,
    title: String,
    artist: String,
) -> CommandResult<Option<Lyrics>> {
    state.get_lyrics(&track_id, &title, &artist)
}

/* -------------------------------------------------------------------------- */
/* Window                                                                      */
/* -------------------------------------------------------------------------- */

#[tauri::command]
pub fn set_notch_state(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    window: tauri::WebviewWindow,
    // `notchState` from the frontend. Tauri matches the argument name exactly,
    // so this parameter must stay camelCase to receive it.
    #[allow(non_snake_case)] notchState: String,
) -> CommandResult<()> {
    eprintln!("[notch] set_notch_state called with '{notchState}'");
    let target = match notchState.as_str() {
        "compact" => crate::types::NotchState::Compact,
        "expanded" => crate::types::NotchState::Expanded,
        other => {
            eprintln!("[notch] unknown state '{other}'");
            return Err(AppError::msg(format!("Unknown notch state '{other}'.")));
        }
    };
    let result = state.set_notch_state(window, target);
    match &result {
        Ok(()) => eprintln!("[notch] resized to {notchState} ok"),
        Err(e) => eprintln!("[notch] resize FAILED: {e}"),
    }
    result
}

#[tauri::command]
pub fn list_monitors(window: tauri::WebviewWindow) -> CommandResult<Vec<MonitorInfo>> {
    crate::windows::notch::list_monitors(&window)
}

#[tauri::command]
pub fn open_devtools(window: tauri::WebviewWindow) -> CommandResult<()> {
    #[cfg(debug_assertions)]
    {
        // `open_devtools` lives on the webview window, not the plain window.
        window.open_devtools();
        Ok(())
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = window;
        Err(AppError::msg("Developer tools are only available in a debug build."))
    }
}

/// A tiny payload used to check that the command bridge works end to end.
#[derive(Serialize)]
pub struct Health {
    pub ok: bool,
    pub version: String,
}

#[tauri::command]
pub fn health() -> Health {
    Health {
        ok: true,
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}
