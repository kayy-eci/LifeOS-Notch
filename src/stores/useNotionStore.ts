import { create } from "zustand";
import { notionApi } from "../services/ipc";
import { onNotionStatus, type Subscription } from "../services/events";
import type { NotionDatabase, NotionStatus } from "../types";

interface NotionStore {
  status: NotionStatus | null;
  databases: NotionDatabase[];
  loadingDatabases: boolean;
  error: string | null;
  /** True while an OAuth round trip is in flight. */
  connecting: boolean;
  /** True while a sync is running. */
  syncing: boolean;
  load: () => Promise<void>;
  configure: (clientId: string, clientSecret: string) => Promise<boolean>;
  connect: () => Promise<void>;
  disconnect: () => Promise<void>;
  loadDatabases: () => Promise<void>;
  selectDatabase: (id: string) => Promise<void>;
  sync: () => Promise<void>;
}

export const useNotionStore = create<NotionStore>((set) => ({
  status: null,
  databases: [],
  loadingDatabases: false,
  error: null,
  connecting: false,
  syncing: false,

  load: async () => {
    try {
      set({ status: await notionApi.status(), error: null });
    } catch (cause) {
      set({ error: cause instanceof Error ? cause.message : String(cause) });
    }
  },

  configure: async (clientId, clientSecret) => {
    set({ error: null });
    try {
      set({ status: await notionApi.configure(clientId, clientSecret) });
      return true;
    } catch (cause) {
      set({ error: cause instanceof Error ? cause.message : String(cause) });
      return false;
    }
  },

  connect: async () => {
    set({ connecting: true, error: null });
    try {
      set({ status: await notionApi.connect(), connecting: false });
    } catch (cause) {
      set({ connecting: false, error: cause instanceof Error ? cause.message : String(cause) });
    }
  },

  disconnect: async () => {
    try {
      set({ status: await notionApi.disconnect(), error: null });
    } catch (cause) {
      set({ error: cause instanceof Error ? cause.message : String(cause) });
    }
  },

  loadDatabases: async () => {
    set({ loadingDatabases: true, error: null });
    try {
      set({ databases: await notionApi.databases(), loadingDatabases: false });
    } catch (cause) {
      set({ loadingDatabases: false, error: cause instanceof Error ? cause.message : String(cause) });
    }
  },

  selectDatabase: async (id) => {
    try {
      set({ status: await notionApi.selectDatabase(id), error: null });
    } catch (cause) {
      set({ error: cause instanceof Error ? cause.message : String(cause) });
    }
  },

  sync: async () => {
    set({ syncing: true, error: null });
    try {
      set({ status: await notionApi.sync(), syncing: false });
    } catch (cause) {
      set({ syncing: false, error: cause instanceof Error ? cause.message : String(cause) });
    }
  },
}));

export function subscribeToNotion(): Subscription {
  return onNotionStatus((status) => {
    useNotionStore.setState({ status });
  });
}
