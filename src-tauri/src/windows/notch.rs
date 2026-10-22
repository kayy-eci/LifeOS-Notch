//! Window placement and sizing for the notch.
//!
//! The notch is a single frameless, transparent, always-on-top window that
//! sits at the top centre of the screen. It has two sizes:
//!
//! * **compact** — a thin bar showing the clock and the current track.
//! * **expanded** — the full panel with the feature navigation.
//!
//! # Sizing and DPI
//!
//! The width and height are *logical* pixels, so the notch looks the same
//! physical size on a 100% display and on a 150% one. The position is
//! computed in *physical* pixels, because that is the coordinate space
//! monitors report their rectangles in, and mixing the two is how notch
//! apps end up off-centre on mixed-DPI setups.
//!
//! The window is positioned before it is resized: moving it can change the
//! scale factor the window is rendered at, and the size conversion depends
//! on that factor.

use tauri::{LogicalSize, Manager, PhysicalPosition, WebviewWindow};

use crate::error::{AppError, AppResult};
use crate::types::{MonitorInfo, NotchState};

/// Compact width in logical pixels.
pub const COMPACT_WIDTH: f64 = 360.0;
/// Compact height in logical pixels.
pub const COMPACT_HEIGHT: f64 = 44.0;

/// Expanded width in logical pixels.
pub const EXPANDED_WIDTH: f64 = 760.0;
/// Expanded height in logical pixels.
pub const EXPANDED_HEIGHT: f64 = 520.0;

/// The dimensions for a notch state.
pub fn dimensions(state: NotchState) -> (f64, f64) {
    match state {
        NotchState::Compact => (COMPACT_WIDTH, COMPACT_HEIGHT),
        NotchState::Expanded => (EXPANDED_WIDTH, EXPANDED_HEIGHT),
    }
}

/// Resizes and repositions the notch for the given state.
pub fn set_state(window: &WebviewWindow, state: NotchState) -> AppResult<()> {
    let (width, height) = dimensions(state);

    // Resize first, then centre. `position_at_top_centre` measures the window
    // with `outer_size()` to compute the offset; running it before the resize
    // centres using the *previous* width, which leaves the notch visibly off
    // centre — most obviously when going from compact to expanded.
    window
        .set_size(LogicalSize::new(width, height))
        .map_err(|e| AppError::msg(format!("Could not resize the notch: {e}")))?;

    position_at_top_centre(window)?;

    // The window must not steal focus when it opens: the user is usually
    // looking at something else. Clicking inside it still works, because the
    // webview takes focus on click.
    if state == NotchState::Expanded {
        let _ = window.set_focus();
    }

    Ok(())
}

/// Places the window at the top centre of the monitor it currently sits on,
/// falling back to the primary monitor.
pub fn position_at_top_centre(window: &WebviewWindow) -> AppResult<()> {
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
        .ok_or_else(|| AppError::msg("No monitor was found."))?;

    let scale = monitor.scale_factor();
    let monitor_position = monitor.position();
    let monitor_size = monitor.size();

    // The window's current logical width decides how wide the notch is on
    // screen. Reading it back keeps the centring correct across the
    // compact/expanded transition.
    let current_size = window
        .outer_size()
        .map(|s| s.to_logical::<f64>(scale))
        .unwrap_or(LogicalSize::new(COMPACT_WIDTH, COMPACT_HEIGHT));
    let window_width_physical = current_size.width * scale;

    let x = monitor_position.x + ((monitor_size.width as f64 - window_width_physical) / 2.0).round() as i32;
    let y = monitor_position.y;

    window
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|e| AppError::msg(format!("Could not move the notch: {e}")))?;
    Ok(())
}

/// Re-places the notch after a display change.
///
/// Called when the window moves, when the scale factor changes, or when the
/// user changes their display arrangement. Without this, a notch created on
/// one monitor can stay behind on a disconnected one.
pub fn handle_display_change(window: &WebviewWindow) {
    if let Err(error) = position_at_top_centre(window) {
        log::warn!("could not re-position the notch: {error}");
    }
}

/// Lists the monitors the notch can be shown on.
pub fn list_monitors(window: &WebviewWindow) -> AppResult<Vec<MonitorInfo>> {
    let mut monitors = Vec::new();
    let primary = window.primary_monitor().ok().flatten();

    for monitor in window.available_monitors().map_err(|e| {
        AppError::msg(format!("Could not read the monitor list: {e}"))
    })? {
        let position = monitor.position();
        let size = monitor.size();
        let is_primary = primary
            .as_ref()
            .map(|p| p.name() == monitor.name())
            .unwrap_or(false);

        monitors.push(MonitorInfo {
            // `name()` returns `Option<&String>`; a monitor with no reported
            // name is still listed, keyed by its position.
            name: monitor
                .name()
                .cloned()
                .unwrap_or_else(|| format!("Display at {},{}", position.x, position.y)),
            width: size.width,
            height: size.height,
            scale_factor: monitor.scale_factor(),
            is_primary,
            x: position.x,
            y: position.y,
        });
    }

    Ok(monitors)
}

/// The monitor the notch is currently on, as physical pixels.
pub fn current_monitor_rect(window: &WebviewWindow) -> AppResult<PhysicalPosition<i32>> {
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
        .ok_or_else(|| AppError::msg("No monitor was found."))?;
    Ok(*monitor.position())
}

/// Makes the window click-through, so the desktop underneath is reachable.
///
/// Used for the parts of the window that carry no controls. The notch itself
/// stays interactive.
pub fn set_click_through(window: &WebviewWindow, ignore: bool) {
    if let Err(error) = window.set_ignore_cursor_events(ignore) {
        log::warn!("could not change the click-through state: {error}");
    }
}

/// Finds the notch window by label.
pub fn notch_window(app: &tauri::AppHandle) -> AppResult<WebviewWindow> {
    app.get_webview_window("notch")
        .ok_or_else(|| AppError::msg("The notch window is not available."))
}
