//! LifeOS Notch — application entry point.
//!
//! # Threads
//!
//! Two background loops run for the life of the app:
//!
//! * **pomodoro ticker** — every 500 ms, checks whether the current phase has
//!   expired. Cheap: it compares two integers. It is what makes the timer
//!   correct while the panel is closed.
//! * **media poller** — every 2 s, refreshes the media session snapshot and
//!   emits an event only when something the user can see has changed.
//!
//! Both stop when the app exits, and both do nothing when there is nothing to
//! do (no running timer, no media session), so an idle notch costs no CPU.

mod commands;
mod db;
mod error;
mod services;
mod state;
mod types;
mod windows;

use std::sync::Arc;
use std::time::Duration;

use tauri::{Emitter, Manager};

use crate::db::Database;
use crate::services::{pomodoro, settings as settings_service};
use crate::state::{events, AppState};
use crate::types::PomodoroStatus;

/// How often the timer checks whether a phase has ended.
const TICK_INTERVAL: Duration = Duration::from_millis(500);
/// How often the media session is refreshed.
const MEDIA_POLL_INTERVAL: Duration = Duration::from_millis(2000);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            // No extra arguments: the app already knows how to start
            // minimised from its own settings.
            None,
        ))
        .setup(|app| {
            let handle = app.handle().clone();

            // The database lives beside the app's other data, under
            // %APPDATA%\com.lifeos.notch.
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("Could not locate the application data folder: {e}"))?;
            std::fs::create_dir_all(&data_dir)?;

            let database = Database::open(&data_dir.join("lifeos.db"))
                .map_err(|e| format!("Could not open the database: {e}"))?;

            let state = AppState::new(handle.clone(), database)
                .map_err(|e| format!("Could not start the application: {e}"))?;
            let state = Arc::new(state);

            // Managed as `Arc<AppState>`. The command signatures must therefore
            // ask for `State<'_, Arc<AppState>>`: Tauri resolves managed state
            // by `TypeId`, and `State<'_, AppState>` would look for a different
            // key, find nothing, and reject the command before its body runs —
            // with no log line and no error surfaced to the UI.
            app.manage(state.clone());

            // Place the notch at the top centre of the primary monitor and
            // make sure it starts in its compact state.
            if let Some(window) = app.get_webview_window("notch") {
                let _ = windows::notch::set_state(&window, types::NotchState::Compact);

                // Keep the notch centred when the display arrangement changes.
                let handle_for_scale = handle.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::ScaleFactorChanged { .. } = event {
                        if let Some(window) = handle_for_scale.get_webview_window("notch") {
                            windows::notch::handle_display_change(&window);
                        }
                    }
                });
            }

            spawn_pomodoro_ticker(handle.clone(), state.clone());
            spawn_media_poller(handle.clone(), state.clone());

            // Apply the saved startup preference on first run so the registry
            // and the setting cannot disagree.
            let settings = settings_service::load(&state.db).unwrap_or_default();
            if settings.start_with_windows {
                use tauri_plugin_autostart::ManagerExt;
                let _ = handle.autolaunch().enable();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::health,
            commands::get_settings,
            commands::update_settings,
            commands::get_pomodoro_state,
            commands::pomodoro_start,
            commands::pomodoro_pause,
            commands::pomodoro_resume,
            commands::pomodoro_skip,
            commands::pomodoro_reset,
            commands::get_water_day,
            commands::add_water,
            commands::remove_water,
            commands::clear_water_day,
            commands::list_tasks,
            commands::create_task,
            commands::update_task,
            commands::delete_task,
            commands::notion_status,
            commands::notion_configure,
            commands::notion_connect,
            commands::notion_disconnect,
            commands::notion_list_databases,
            commands::notion_select_database,
            commands::notion_sync,
            commands::media_current_session,
            commands::media_command,
            commands::media_seek,
            commands::get_lyrics,
            commands::set_notch_state,
            commands::list_monitors,
            commands::open_devtools,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LifeOS Notch");
}

/// Checks the timer twice a second and announces a finished phase.
fn spawn_pomodoro_ticker(handle: tauri::AppHandle, state: Arc<AppState>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(TICK_INTERVAL);

        let settings = match state.settings() {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Read the current state, advance it if the deadline has passed, and
        // only take the write lock when something actually changed.
        let transition = {
            let mut guard = match state.pomodoro.lock() {
                Ok(g) => g,
                Err(_) => continue,
            };
            let now = types::now_ms();
            let transition = pomodoro::advance_if_expired(&mut guard, &settings, now);
            if transition.is_some() {
                let _ = pomodoro::save(&state.db, &guard);
                let _ = handle.emit(events::POMODORO_STATE, guard.clone());
            }
            transition
        };

        if let Some(transition) = transition {
            state.on_phase_finished(&transition);
        } else {
            // Nothing ended. Still emit a state update once a second while
            // running so the countdown in the UI stays in step with the
            // backend's clock, which is the authority on remaining time.
            if let Ok(guard) = state.pomodoro.lock() {
                if guard.status == PomodoroStatus::Running {
                    let _ = handle.emit(events::POMODORO_STATE, guard.clone());
                }
            }
        }
    });
}

/// Refreshes the media session and emits only when it changed.
fn spawn_media_poller(handle: tauri::AppHandle, state: Arc<AppState>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(MEDIA_POLL_INTERVAL);

        match state.media.poll() {
            Ok(Some(session)) => {
                let _ = handle.emit(events::MEDIA_SESSION, session);
            }
            // No change, or no session: nothing to do, and nothing emitted.
            Ok(None) => {}
            Err(error) => {
                // A media failure must never take the app down; the panel
                // simply keeps showing the last known state.
                log::debug!("media poll failed: {error}");
            }
        }
    });
}
