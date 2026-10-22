/**
 * Typed wrappers around the Tauri command boundary.
 *
 * Every backend call goes through here so that:
 *  - the command name and its argument shape are checked at compile time;
 *  - errors arrive as `Error` instances with the backend's message intact,
 *    rather than as raw `unknown`;
 *  - the frontend never touches `invoke` directly.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  Lyrics,
  MediaCommand,
  MediaSession,
  MonitorInfo,
  NotionDatabase,
  NotionStatus,
  PomodoroState,
  Task,
  WaterDay,
} from "../types";

/**
 * Normalises the error shapes Tauri can throw. `invoke` rejects with the
 * serialised `Err` value from Rust, which for us is always a string message.
 */
async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (cause) {
    throw new Error(describeError(cause));
  }
}

export function describeError(cause: unknown): string {
  if (cause instanceof Error) return cause.message;
  if (typeof cause === "string") return cause;
  if (cause && typeof cause === "object" && "message" in cause) {
    return String((cause as { message: unknown }).message);
  }
  return "Unknown error";
}

/* -------------------------------------------------------------------------- */
/* Settings                                                                    */
/* -------------------------------------------------------------------------- */

export const settingsApi = {
  get: () => call<AppSettings>("get_settings"),
  update: (patch: Partial<AppSettings>) => call<AppSettings>("update_settings", { patch }),
};

/* -------------------------------------------------------------------------- */
/* Pomodoro                                                                    */
/* -------------------------------------------------------------------------- */

export const pomodoroApi = {
  get: () => call<PomodoroState>("get_pomodoro_state"),
  start: () => call<PomodoroState>("pomodoro_start"),
  pause: () => call<PomodoroState>("pomodoro_pause"),
  resume: () => call<PomodoroState>("pomodoro_resume"),
  skip: () => call<PomodoroState>("pomodoro_skip"),
  reset: () => call<PomodoroState>("pomodoro_reset"),
};

/* -------------------------------------------------------------------------- */
/* Water                                                                       */
/* -------------------------------------------------------------------------- */

export const waterApi = {
  getDay: (date?: string) => call<WaterDay>("get_water_day", { date: date ?? null }),
  add: (amountMl: number) => call<WaterDay>("add_water", { amountMl }),
  remove: (id: string) => call<WaterDay>("remove_water", { id }),
  clearDay: (date?: string) => call<WaterDay>("clear_water_day", { date: date ?? null }),
};

/* -------------------------------------------------------------------------- */
/* Tasks                                                                       */
/* -------------------------------------------------------------------------- */

export const tasksApi = {
  list: () => call<Task[]>("list_tasks"),
  create: (input: { title: string; dueAt?: number | null; description?: string | null }) =>
    call<Task>("create_task", { input }),
  update: (
    id: string,
    patch: Partial<{ title: string; done: boolean; dueAt: number | null; description: string | null }>,
  ) => call<Task>("update_task", { id, patch }),
  remove: (id: string) => call<void>("delete_task", { id }),
};

/* -------------------------------------------------------------------------- */
/* Notion                                                                      */
/* -------------------------------------------------------------------------- */

export const notionApi = {
  status: () => call<NotionStatus>("notion_status"),
  /** Stores the OAuth client id/secret in the OS credential store. */
  configure: (clientId: string, clientSecret: string) => call<NotionStatus>("notion_configure", {
    clientId,
    clientSecret,
  }),
  /** Opens the browser at Notion's consent screen and waits for the callback. */
  connect: () => call<NotionStatus>("notion_connect"),
  disconnect: () => call<NotionStatus>("notion_disconnect"),
  databases: () => call<NotionDatabase[]>("notion_list_databases"),
  selectDatabase: (id: string) => call<NotionStatus>("notion_select_database", { id }),
  sync: () => call<NotionStatus>("notion_sync"),
};

/* -------------------------------------------------------------------------- */
/* Media                                                                       */
/* -------------------------------------------------------------------------- */

export const mediaApi = {
  current: () => call<MediaSession | null>("media_current_session"),
  command: (command: MediaCommand) => call<MediaSession | null>("media_command", { command }),
  seek: (positionSeconds: number) => call<MediaSession | null>("media_seek", { positionSeconds }),
};

/* -------------------------------------------------------------------------- */
/* Lyrics                                                                      */
/* -------------------------------------------------------------------------- */

export const lyricsApi = {
  /** `trackId` is the cache key; the backend resolves it against its cache first. */
  get: (trackId: string, title: string, artist: string) =>
    call<Lyrics | null>("get_lyrics", { trackId, title, artist }),
};

/* -------------------------------------------------------------------------- */
/* Window                                                                      */
/* -------------------------------------------------------------------------- */

export const windowApi = {
  setNotchState: (notchState: "compact" | "expanded") =>
    call<void>("set_notch_state", { notchState }),
  listMonitors: () => call<MonitorInfo[]>("list_monitors"),
  openDevTools: () => call<void>("open_devtools"),
};
