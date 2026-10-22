import { create } from "zustand";
import { tasksApi } from "../services/ipc";
import type { Task } from "../types";

interface TasksStore {
  tasks: Task[];
  loading: boolean;
  error: string | null;
  load: () => Promise<void>;
  create: (title: string) => Promise<void>;
  toggle: (task: Task) => Promise<void>;
  rename: (task: Task, title: string) => Promise<void>;
  remove: (task: Task) => Promise<void>;
}

export const useTasksStore = create<TasksStore>((set, get) => ({
  tasks: [],
  loading: false,
  error: null,

  load: async () => {
    set({ loading: true, error: null });
    try {
      set({ tasks: await tasksApi.list(), loading: false });
    } catch (cause) {
      set({ loading: false, error: cause instanceof Error ? cause.message : String(cause) });
    }
  },

  create: async (title) => {
    const trimmed = title.trim();
    if (!trimmed) return;
    try {
      const task = await tasksApi.create({ title: trimmed });
      set({ tasks: [task, ...get().tasks], error: null });
    } catch (cause) {
      set({ error: cause instanceof Error ? cause.message : String(cause) });
    }
  },

  toggle: async (task) => {
    try {
      const updated = await tasksApi.update(task.id, { done: !task.done });
      set({ tasks: get().tasks.map((t) => (t.id === updated.id ? updated : t)), error: null });
    } catch (cause) {
      set({ error: cause instanceof Error ? cause.message : String(cause) });
    }
  },

  rename: async (task, title) => {
    const trimmed = title.trim();
    if (!trimmed || trimmed === task.title) return;
    try {
      const updated = await tasksApi.update(task.id, { title: trimmed });
      set({ tasks: get().tasks.map((t) => (t.id === updated.id ? updated : t)), error: null });
    } catch (cause) {
      set({ error: cause instanceof Error ? cause.message : String(cause) });
    }
  },

  remove: async (task) => {
    // Remove locally first so the list responds immediately; the backend
    // call then confirms. A failure restores the row rather than dropping it.
    const previous = get().tasks;
    set({ tasks: previous.filter((t) => t.id !== task.id) });
    try {
      await tasksApi.remove(task.id);
      set({ error: null });
    } catch (cause) {
      set({
        tasks: previous,
        error: cause instanceof Error ? cause.message : String(cause),
      });
    }
  },
}));
