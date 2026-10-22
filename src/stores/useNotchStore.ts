import { create } from "zustand";

/**
 * Which panel is open in the expanded notch.
 *
 * A single value rather than a stack: the notch has room for one panel at a
 * time, and nesting would only add navigation the user has to undo.
 */
export type PanelId = "music" | "lyrics" | "pomodoro" | "water" | "todos" | "settings";

interface NotchState {
  /** True while the pointer is inside the notch or the panel is pinned open. */
  expanded: boolean;
  /** Set when the user clicks the notch; keeps it open until dismissed. */
  pinned: boolean;
  activePanel: PanelId;
  /** Set while a panel is loading its first payload. */
  booting: boolean;
  setExpanded: (expanded: boolean) => void;
  setPinned: (pinned: boolean) => void;
  setActivePanel: (panel: PanelId) => void;
  setBooting: (booting: boolean) => void;
  /** A runtime failure worth showing, e.g. the window refusing to resize. */
  error: string | null;
  setError: (error: string | null) => void;
  collapse: () => void;
}

export const useNotchStore = create<NotchState>((set) => ({
  expanded: false,
  pinned: false,
  activePanel: "music",
  booting: true,
  error: null,
  setExpanded: (expanded) => set({ expanded }),
  setPinned: (pinned) => set({ pinned }),
  setActivePanel: (activePanel) => set({ activePanel }),
  setBooting: (booting) => set({ booting }),
  setError: (error) => set({ error }),
  collapse: () => set({ expanded: false, pinned: false }),
}));
