use serde::{Deserialize, Serialize};

/// The data contract shared with the React frontend.
///
/// Field names are `snake_case` and match `src/types/index.ts` one for one.
/// Timestamps are Unix epoch milliseconds, never ISO strings, so that the
/// frontend never has to guess a time zone.

pub type Millis = i64;

pub fn now_ms() -> Millis {
    chrono::Utc::now().timestamp_millis()
}

/* -------------------------------------------------------------------------- */
/* Settings                                                                    */
/* -------------------------------------------------------------------------- */

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct AppSettings {
    pub focus_minutes: u32,
    pub short_break_minutes: u32,
    pub long_break_minutes: u32,
    pub long_break_after: u32,
    pub auto_start_breaks: bool,
    pub auto_start_focus: bool,

    pub water_target_ml: u32,
    pub water_serving_ml: u32,
    pub water_reminder_enabled: bool,
    pub water_reminder_interval_minutes: u32,
    pub water_reminder_start_hour: u32,
    pub water_reminder_end_hour: u32,

    pub notifications_enabled: bool,
    pub notify_pomodoro: bool,
    pub notify_water: bool,
    pub notify_tasks: bool,

    pub start_with_windows: bool,
    pub launch_minimised: bool,
    pub hide_on_fullscreen: bool,

    pub notion_database_id: Option<String>,
    pub notion_database_name: Option<String>,
    pub notion_property_map: Option<NotionPropertyMap>,

    pub updated_at: Millis,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            focus_minutes: 25,
            short_break_minutes: 5,
            long_break_minutes: 15,
            long_break_after: 4,
            auto_start_breaks: true,
            auto_start_focus: false,

            water_target_ml: 2000,
            water_serving_ml: 250,
            water_reminder_enabled: false,
            water_reminder_interval_minutes: 60,
            water_reminder_start_hour: 9,
            water_reminder_end_hour: 18,

            notifications_enabled: true,
            notify_pomodoro: true,
            notify_water: true,
            notify_tasks: true,

            start_with_windows: false,
            launch_minimised: true,
            hide_on_fullscreen: true,

            notion_database_id: None,
            notion_database_name: None,
            notion_property_map: None,

            updated_at: 0,
        }
    }
}

/// How the selected Notion database maps onto task fields.
///
/// Stored as JSON so the mapping can evolve without a schema migration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NotionPropertyMap {
    pub title: String,
    pub done: Option<String>,
    pub due: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub done_values: Vec<String>,
}

/* -------------------------------------------------------------------------- */
/* Pomodoro                                                                    */
/* -------------------------------------------------------------------------- */

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PomodoroMode {
    Focus,
    ShortBreak,
    LongBreak,
}

impl PomodoroMode {
    pub fn as_str(self) -> &'static str {
        match self {
            PomodoroMode::Focus => "focus",
            PomodoroMode::ShortBreak => "short_break",
            PomodoroMode::LongBreak => "long_break",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PomodoroStatus {
    Idle,
    Running,
    Paused,
}

/// The authoritative timer state.
///
/// The deadline is stored, not a remaining count, so the timer stays correct
/// when the UI is idle, the machine sleeps, or the panel is closed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PomodoroState {
    pub mode: PomodoroMode,
    pub status: PomodoroStatus,
    /// Epoch millis at which the current phase ends. `None` when not running.
    pub ends_at: Option<Millis>,
    /// Seconds left when the timer was paused. `None` when not paused.
    pub remaining_seconds: Option<u32>,
    pub completed_focus_sessions: u32,
    pub completed_today: u32,
    pub completed_total: u32,
    pub started_at: Option<Millis>,
}

/* -------------------------------------------------------------------------- */
/* Water                                                                       */
/* -------------------------------------------------------------------------- */

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WaterEntry {
    pub id: String,
    pub amount_ml: u32,
    pub logged_at: Millis,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WaterDay {
    /// Local calendar date, `YYYY-MM-DD`.
    pub date: String,
    pub target_ml: u32,
    pub total_ml: u32,
    pub goal_reached: bool,
    pub entries: Vec<WaterEntry>,
}

/* -------------------------------------------------------------------------- */
/* Tasks                                                                       */
/* -------------------------------------------------------------------------- */

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskSyncState {
    Local,
    Pending,
    Synced,
    Conflict,
    Failed,
}

impl TaskSyncState {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskSyncState::Local => "local",
            TaskSyncState::Pending => "pending",
            TaskSyncState::Synced => "synced",
            TaskSyncState::Conflict => "conflict",
            TaskSyncState::Failed => "failed",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "pending" => TaskSyncState::Pending,
            "synced" => TaskSyncState::Synced,
            "conflict" => TaskSyncState::Conflict,
            "failed" => TaskSyncState::Failed,
            _ => TaskSyncState::Local,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Task {
    pub id: String,
    pub notion_page_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub done: bool,
    pub due_at: Option<Millis>,
    pub created_at: Millis,
    pub updated_at: Millis,
    pub remote_updated_at: Option<Millis>,
    pub sync_state: TaskSyncState,
    pub pending_ops: u32,
}

/// A change made locally that still has to reach Notion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncOperation {
    pub id: i64,
    pub task_id: String,
    /// `create`, `update` or `delete`.
    pub kind: String,
    /// The payload to send, as JSON.
    pub payload: String,
    pub attempts: u32,
    pub last_error: Option<String>,
    pub created_at: Millis,
}

/* -------------------------------------------------------------------------- */
/* Notion                                                                      */
/* -------------------------------------------------------------------------- */

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotionConnectionState {
    Disconnected,
    NeedsAuth,
    Connected,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NotionStatus {
    pub state: NotionConnectionState,
    pub message: Option<String>,
    pub workspace_name: Option<String>,
    pub database_id: Option<String>,
    pub database_name: Option<String>,
    pub last_sync_at: Option<Millis>,
    pub pending_changes: u32,
    /// True when an OAuth client id and secret have been stored.
    pub configured: bool,
}

impl Default for NotionStatus {
    fn default() -> Self {
        Self {
            state: NotionConnectionState::Disconnected,
            message: None,
            workspace_name: None,
            database_id: None,
            database_name: None,
            last_sync_at: None,
            pending_changes: 0,
            configured: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotionPropertyKind {
    Title,
    RichText,
    Status,
    Select,
    Date,
    Checkbox,
    Number,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NotionProperty {
    pub name: String,
    pub kind: NotionPropertyKind,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NotionDatabase {
    pub id: String,
    pub title: String,
    pub properties: Vec<NotionProperty>,
}

/* -------------------------------------------------------------------------- */
/* Media                                                                       */
/* -------------------------------------------------------------------------- */

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
    Changing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MediaSession {
    pub app_id: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// Album art as a `data:` URL, or `None`.
    pub artwork: Option<String>,
    pub status: PlaybackStatus,
    pub position_seconds: f64,
    pub duration_seconds: Option<f64>,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_skip_next: bool,
    pub can_skip_previous: bool,
    pub can_seek: bool,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaCommand {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    Stop,
}

/* -------------------------------------------------------------------------- */
/* Lyrics                                                                      */
/* -------------------------------------------------------------------------- */

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LyricLine {
    pub time_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Lyrics {
    pub track_id: String,
    pub title: String,
    pub artist: String,
    pub synced: bool,
    pub lines: Vec<LyricLine>,
    pub source_url: Option<String>,
}

/* -------------------------------------------------------------------------- */
/* Window                                                                      */
/* -------------------------------------------------------------------------- */

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotchState {
    Compact,
    Expanded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MonitorInfo {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    pub is_primary: bool,
    pub x: i32,
    pub y: i32,
}
