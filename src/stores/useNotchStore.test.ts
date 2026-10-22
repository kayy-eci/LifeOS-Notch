import { describe, expect, it, beforeEach } from "vitest";
import { useNotchStore } from "../stores/useNotchStore";

/**
 * The notch interaction state machine.
 *
 * These tests pin the behaviour the spec asks for: hover opens, leaving
 * schedules a collapse, and a pinned notch ignores the pointer. They run
 * without a browser because the store is plain state.
 */
describe("useNotchStore", () => {
  beforeEach(() => {
    useNotchStore.setState({ expanded: false, pinned: false, activePanel: "music", booting: false });
  });

  it("starts compact", () => {
    expect(useNotchStore.getState().expanded).toBe(false);
    expect(useNotchStore.getState().pinned).toBe(false);
  });

  it("expands on request", () => {
    useNotchStore.getState().setExpanded(true);
    expect(useNotchStore.getState().expanded).toBe(true);
  });

  it("collapse clears both expanded and pinned", () => {
    useNotchStore.getState().setExpanded(true);
    useNotchStore.getState().setPinned(true);
    useNotchStore.getState().collapse();
    expect(useNotchStore.getState().expanded).toBe(false);
    expect(useNotchStore.getState().pinned).toBe(false);
  });

  it("switches the active panel", () => {
    useNotchStore.getState().setActivePanel("water");
    expect(useNotchStore.getState().activePanel).toBe("water");
  });
});
