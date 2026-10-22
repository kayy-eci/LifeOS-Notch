import { create } from "zustand";
import { waterApi } from "../services/ipc";
import { onWaterDay, type Subscription } from "../services/events";
import type { WaterDay } from "../types";

interface WaterStore {
  day: WaterDay | null;
  loading: boolean;
  error: string | null;
  load: () => Promise<void>;
  add: (amountMl: number) => Promise<void>;
  remove: (id: string) => Promise<void>;
  clear: () => Promise<void>;
}

export const useWaterStore = create<WaterStore>((set) => ({
  day: null,
  loading: false,
  error: null,

  load: async () => {
    set({ loading: true, error: null });
    try {
      set({ day: await waterApi.getDay(), loading: false });
    } catch (cause) {
      set({ loading: false, error: cause instanceof Error ? cause.message : String(cause) });
    }
  },

  add: async (amountMl) => {
    try {
      set({ day: await waterApi.add(amountMl), error: null });
    } catch (cause) {
      set({ error: cause instanceof Error ? cause.message : String(cause) });
    }
  },

  remove: async (id) => {
    try {
      set({ day: await waterApi.remove(id), error: null });
    } catch (cause) {
      set({ error: cause instanceof Error ? cause.message : String(cause) });
    }
  },

  clear: async () => {
    try {
      set({ day: await waterApi.clearDay(), error: null });
    } catch (cause) {
      set({ error: cause instanceof Error ? cause.message : String(cause) });
    }
  },
}));

export function subscribeToWater(): Subscription {
  return onWaterDay((day) => {
    useWaterStore.setState({ day });
  });
}
