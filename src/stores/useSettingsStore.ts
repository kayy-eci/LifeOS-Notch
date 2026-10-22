import { create } from "zustand";
import { settingsApi } from "../services/ipc";
import type { AppSettings } from "../types";

interface SettingsState {
  settings: AppSettings | null;
  loading: boolean;
  error: string | null;
  saving: boolean;
  load: () => Promise<void>;
  update: (patch: Partial<AppSettings>) => Promise<void>;
}

export const useSettingsStore = create<SettingsState>((set, get) => ({
  settings: null,
  loading: false,
  error: null,
  saving: false,

  load: async () => {
    set({ loading: true, error: null });
    try {
      const settings = await settingsApi.get();
      set({ settings, loading: false });
    } catch (cause) {
      set({ loading: false, error: cause instanceof Error ? cause.message : String(cause) });
    }
  },

  update: async (patch) => {
    set({ saving: true, error: null });
    try {
      // The backend echoes the merged record, so the UI never has to guess
      // what the final value is after a patch.
      const settings = await settingsApi.update(patch);
      set({ settings, saving: false });
    } catch (cause) {
      set({ saving: false, error: cause instanceof Error ? cause.message : String(cause) });
      // Re-read so the form shows the values that are actually stored.
      void get().load();
    }
  },
}));
