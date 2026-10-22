/**
 * Typed event channel between Rust and React.
 *
 * The backend pushes state changes here so the UI never has to poll. Every
 * payload is the same shape the corresponding `invoke` command returns, which
 * keeps a single source of truth for the data model.
 *
 * Each subscribe call resolves to an unsubscribe function. The `Promise` is
 * intentional: `listen` is async, and the caller must be able to cancel a
 * subscription that has not finished registering yet.
 */
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { MediaSession, NotionStatus, PomodoroState, WaterDay } from "../types";

export const EVENTS = {
  mediaSession: "lifeos://media-session",
  pomodoroState: "lifeos://pomodoro-state",
  notionStatus: "lifeos://notion-status",
  waterDay: "lifeos://water-day",
} as const;

/** A subscription that can be cancelled once it has registered. */
export type Subscription = Promise<UnlistenFn>;

export const onMediaSession = (fn: (session: MediaSession | null) => void): Subscription =>
  listen<MediaSession | null>(EVENTS.mediaSession, (e) => fn(e.payload));

export const onPomodoroState = (fn: (state: PomodoroState) => void): Subscription =>
  listen<PomodoroState>(EVENTS.pomodoroState, (e) => fn(e.payload));

export const onNotionStatus = (fn: (status: NotionStatus) => void): Subscription =>
  listen<NotionStatus>(EVENTS.notionStatus, (e) => fn(e.payload));

export const onWaterDay = (fn: (day: WaterDay) => void): Subscription =>
  listen<WaterDay>(EVENTS.waterDay, (e) => fn(e.payload));
