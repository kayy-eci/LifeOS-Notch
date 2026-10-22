import { create } from "zustand";
import { pomodoroApi } from "../services/ipc";
import { onPomodoroState, type Subscription } from "../services/events";
import type { PomodoroState } from "../types";

interface PomodoroStore {
  state: PomodoroState | null;
  error: string | null;
  busy: boolean;
  load: () => Promise<void>;
  /** Start the current phase, or resume it if it was paused. */
  start: () => Promise<void>;
  pause: () => Promise<void>;
  skip: () => Promise<void>;
  reset: () => Promise<void>;
}

export const usePomodoroStore = create<PomodoroStore>((set) => ({
  state: null,
  error: null,
  busy: false,

  load: async () => {
    try {
      set({ state: await pomodoroApi.get(), error: null });
    } catch (cause) {
      set({ error: cause instanceof Error ? cause.message : String(cause) });
    }
  },

  start: async () => {
    set({ busy: true });
    try {
      set({ state: await pomodoroApi.start(), busy: false, error: null });
    } catch (cause) {
      set({ busy: false, error: cause instanceof Error ? cause.message : String(cause) });
    }
  },

  pause: async () => {
    set({ busy: true });
    try {
      set({ state: await pomodoroApi.pause(), busy: false, error: null });
    } catch (cause) {
      set({ busy: false, error: cause instanceof Error ? cause.message : String(cause) });
    }
  },

  skip: async () => {
    set({ busy: true });
    try {
      set({ state: await pomodoroApi.skip(), busy: false, error: null });
    } catch (cause) {
      set({ busy: false, error: cause instanceof Error ? cause.message : String(cause) });
    }
  },

  reset: async () => {
    set({ busy: true });
    try {
      set({ state: await pomodoroApi.reset(), busy: false, error: null });
    } catch (cause) {
      set({ busy: false, error: cause instanceof Error ? cause.message : String(cause) });
    }
  },
}));

/**
 * Subscribes to backend state changes. Called once from the app shell.
 *
 * The backend owns the timer: it holds the deadline and advances phases even
 * while the panel is closed, so the UI only ever renders what the backend
 * reports.
 */
export function subscribeToPomodoro(): Subscription {
  return onPomodoroState((state) => {
    usePomodoroStore.setState({ state });
  });
}
