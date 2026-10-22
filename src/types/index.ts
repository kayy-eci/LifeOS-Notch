/**
 * The data contract between the React frontend and the Rust backend.
 *
 * Every type here has a mirror in `src-tauri/src/types.rs`. Field names are
 * `snake_case` on purpose: Rust serialises with `serde` defaults and the
 * frontend would otherwise need a translation layer for every payload.
 *
 * Rules for this file:
 *  - Timestamps are Unix epoch milliseconds (`number`). Never ISO strings.
 *  - `null` means "the value does not exist", never "unknown".
 *  - Secrets never appear here. The frontend only ever sees a connection
 *    *status*, never a token.
 */

/* -------------------------------------------------------------------------- */
/* Settings                                                                    */
/* -------------------------------------------------------------------------- */

export interface AppSettings {
  /* Pomodoro */
  focus_minutes: number;
  short_break_minutes: number;
  long_break_minutes: number;
  long_break_after: number;
  auto_start_breaks: boolean;
  auto_start_focus: boolean;

  /* Water */
  water_target_ml: number;
  water_serving_ml: number;
  water_reminder_enabled: boolean;
  water_reminder_interval_minutes: number;
  water_reminder_start_hour: number;
  water_reminder_end_hour: number;

  /* Notifications */
  notifications_enabled: boolean;
  notify_pomodoro: boolean;
  notify_water: boolean;
  notify_tasks: boolean;

  /* Application */
  start_with_windows: boolean;
  launch_minimised: boolean;
  hide_on_fullscreen: boolean;

  /* Notion — credentials live in the OS credential store, not here. */
  notion_database_id: string | null;
  notion_database_name: string | null;
  notion_property_map: NotionPropertyMap | null;

  /** Wall clock time the app last wrote settings, epoch millis. */
  updated_at: number;
}

/** How Notion database properties map onto task fields. */
export interface NotionPropertyMap {
  title: string;
  done: string | null;
  due: string | null;
  description: string | null;
  /** Notion `status` values that mean "done". */
  done_values: string[];
}

/* -------------------------------------------------------------------------- */
/* Pomodoro                                                                    */
/* -------------------------------------------------------------------------- */

export type PomodoroMode = "focus" | "short_break" | "long_break";
export type PomodoroStatus = "idle" | "running" | "paused";

export interface PomodoroState {
  mode: PomodoroMode;
  status: PomodoroStatus;
  /**
   * Epoch millis at which the current phase ends, or `null` when the timer is
   * not running. The UI derives remaining time from this rather than
   * counting down, so the timer stays accurate while the panel is closed.
   */
  ends_at: number | null;
  /** Seconds left at the moment the timer was paused; `null` when not paused. */
  remaining_seconds: number | null;
  /** Focus sessions completed since the last long break. */
  completed_focus_sessions: number;
  /** Focus sessions completed today. */
  completed_today: number;
  /** Total focus sessions completed over the life of the app. */
  completed_total: number;
  /** Epoch millis when the running phase started. */
  started_at: number | null;
}

/* -------------------------------------------------------------------------- */
/* Water                                                                       */
/* -------------------------------------------------------------------------- */

export interface WaterEntry {
  id: string;
  amount_ml: number;
  /** Epoch millis. */
  logged_at: number;
}

export interface WaterDay {
  /** Local calendar date, `YYYY-MM-DD`. */
  date: string;
  target_ml: number;
  total_ml: number;
  goal_reached: boolean;
  entries: WaterEntry[];
}

/* -------------------------------------------------------------------------- */
/* Tasks                                                                       */
/* -------------------------------------------------------------------------- */

export type TaskSyncState =
  /** Only exists locally; nothing has been pushed to Notion yet. */
  | "local"
  /** A local change is queued for Notion. */
  | "pending"
  /** Local and Notion agree. */
  | "synced"
  /** Both sides changed since the last sync. */
  | "conflict"
  /** The last push failed; the operation is still queued. */
  | "failed";

export interface Task {
  /** Stable local identifier (UUID v4). */
  id: string;
  /** Notion page id, once the task has been pushed. */
  notion_page_id: string | null;
  title: string;
  description: string | null;
  done: boolean;
  /** Epoch millis, or `null` when the task has no due date. */
  due_at: number | null;
  /** Epoch millis. */
  created_at: number;
  /** Epoch millis of the last local change. */
  updated_at: number;
  /** Last known Notion `last_edited_time`, epoch millis. */
  remote_updated_at: number | null;
  sync_state: TaskSyncState;
  /** Number of queued operations for this task. */
  pending_ops: number;
}

/* -------------------------------------------------------------------------- */
/* Notion                                                                      */
/* -------------------------------------------------------------------------- */

export type NotionConnectionState =
  | "disconnected"
  | "needs_auth"
  | "connected"
  | "error";

export interface NotionStatus {
  state: NotionConnectionState;
  /** Human-readable reason when `state` is `error` or `needs_auth`. */
  message: string | null;
  workspace_name: string | null;
  database_id: string | null;
  database_name: string | null;
  /** Epoch millis of the last successful sync. */
  last_sync_at: number | null;
  /** Operations waiting in the offline queue. */
  pending_changes: number;
  /** True when an OAuth client id/secret has been stored. */
  configured: boolean;
}

export type NotionPropertyKind =
  | "title"
  | "rich_text"
  | "status"
  | "select"
  | "date"
  | "checkbox"
  | "number"
  | "unsupported";

export interface NotionProperty {
  name: string;
  kind: NotionPropertyKind;
  /** For `status`/`select`: the option names the property accepts. */
  options: string[];
}

export interface NotionDatabase {
  id: string;
  title: string;
  properties: NotionProperty[];
}

/* -------------------------------------------------------------------------- */
/* Media session (Windows GSMTC)                                               */
/* -------------------------------------------------------------------------- */

export type PlaybackStatus = "playing" | "paused" | "stopped" | "changing";

export interface MediaSession {
  /** Windows application user model id of the owning app. */
  app_id: string;
  title: string | null;
  artist: string | null;
  album: string | null;
  /** Album art as a `data:` URL, or `null` when the app exposes none. */
  artwork: string | null;
  status: PlaybackStatus;
  position_seconds: number;
  /** `null` when the app does not report a duration. */
  duration_seconds: number | null;
  can_play: boolean;
  can_pause: boolean;
  can_skip_next: boolean;
  can_skip_previous: boolean;
  can_seek: boolean;
}

export type MediaCommand = "play" | "pause" | "toggle" | "next" | "previous" | "stop";

/* -------------------------------------------------------------------------- */
/* Lyrics                                                                      */
/* -------------------------------------------------------------------------- */

export interface LyricLine {
  /** Offset from the start of the track, milliseconds. */
  time_ms: number;
  text: string;
}

export interface Lyrics {
  /** Cache key: artist + title, normalised. */
  track_id: string;
  title: string;
  artist: string;
  /** True when at least one line carries a timestamp. */
  synced: boolean;
  lines: LyricLine[];
  source_url: string | null;
}

/* -------------------------------------------------------------------------- */
/* Window                                                                      */
/* -------------------------------------------------------------------------- */

export type NotchState = "compact" | "expanded";

export interface MonitorInfo {
  name: string;
  width: number;
  height: number;
  scale_factor: number;
  is_primary: boolean;
  /** Origin of the monitor in virtual-desktop coordinates. */
  x: number;
  y: number;
}
