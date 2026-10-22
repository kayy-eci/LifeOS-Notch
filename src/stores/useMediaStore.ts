import { create } from "zustand";
import { mediaApi } from "../services/ipc";
import { onMediaSession, type Subscription } from "../services/events";
import type { MediaCommand, MediaSession } from "../types";

interface MediaStore {
  session: MediaSession | null;
  error: string | null;
  /**
   * Playback position in seconds, advanced locally between backend updates so
   * the progress bar moves smoothly. The backend remains the source of truth
   * and corrects this value on every push.
   */
  position: number;
  load: () => Promise<void>;
  command: (command: MediaCommand) => Promise<void>;
  seek: (positionSeconds: number) => Promise<void>;
  /** Applies a backend push, keeping the local position in step. */
  apply: (session: MediaSession | null) => void;
}

export const useMediaStore = create<MediaStore>((set, get) => ({
  session: null,
  error: null,
  position: 0,

  load: async () => {
    try {
      const session = await mediaApi.current();
      get().apply(session);
      set({ error: null });
    } catch (cause) {
      set({ error: cause instanceof Error ? cause.message : String(cause) });
    }
  },

  command: async (command) => {
    try {
      const session = await mediaApi.command(command);
      get().apply(session);
      set({ error: null });
    } catch (cause) {
      set({ error: cause instanceof Error ? cause.message : String(cause) });
    }
  },

  seek: async (positionSeconds) => {
    try {
      await mediaApi.seek(positionSeconds);
      set({ position: positionSeconds, error: null });
    } catch (cause) {
      set({ error: cause instanceof Error ? cause.message : String(cause) });
    }
  },

  apply: (session) => {
    set({ session, position: session ? session.position_seconds : 0 });
  },
}));

export function subscribeToMedia(): Subscription {
  return onMediaSession((session) => {
    useMediaStore.getState().apply(session);
  });
}
