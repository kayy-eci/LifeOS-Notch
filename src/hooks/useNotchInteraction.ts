import { useCallback, useEffect, useRef } from "react";
import { useNotchStore } from "../stores/useNotchStore";
import { windowApi } from "../services/ipc";

/**
 * Centralised expand/collapse behaviour for the notch.
 *
 * Every hover and click in the app funnels through this hook — no component
 * attaches its own `onMouseEnter` to change the notch state. That is what
 * prevents the classic notch bug where two independent handlers disagree and
 * the panel flickers or collapses while the pointer is still inside it.
 *
 * The rules:
 *
 *  - Entering the notch expands it.
 *  - Leaving schedules a collapse after `COLLAPSE_DELAY_MS`. The delay exists
 *    because the pointer crosses the gap between the compact bar and the
 *    expanded panel; collapsing immediately makes that movement impossible.
 *  - Re-entering cancels a scheduled collapse. This is the "cancellation" the
 *    spec asks for, and it is why the timer lives in a ref rather than in
 *    component state: re-entering must clear the *previous* timer, not queue
 *    another one.
 *  - Clicking pins the notch open. A pinned notch ignores pointer-leave, so a
 *    user who is typing into the to-do field cannot lose the panel by moving
 *    the mouse. Clicking the compact bar again, pressing Escape, or clicking
 *    outside unpins it.
 *
 * Window resizing is driven from here too: the webview is resized to match the
 * state, so the expanded panel is never clipped by a 44px-tall window.
 */

/** How long the pointer may be outside before the notch collapses. */
export const COLLAPSE_DELAY_MS = 420;

export function useNotchInteraction() {
  const expanded = useNotchStore((s) => s.expanded);
  const pinned = useNotchStore((s) => s.pinned);
  const setExpanded = useNotchStore((s) => s.setExpanded);

  // A single timer for the pending collapse. Held in a ref so that clearing
  // it does not trigger a re-render, and so a stale closure cannot fire.
  const collapseTimer = useRef<number | null>(null);

  // Ignore leave events for a moment after the window is resized.
  //
  // Resizing the native window moves it under the cursor, and the webview
  // reports that as the pointer leaving and re-entering. Without this guard
  // the scheduled collapse from the spurious leave fires shortly after the
  // expand, closing the panel the user just opened — the notch appears to
  // flash open and shut. The grace period covers the resize round trip.
  const ignoreLeaveUntil = useRef(0);

  const clearCollapseTimer = useCallback(() => {
    if (collapseTimer.current !== null) {
      window.clearTimeout(collapseTimer.current);
      collapseTimer.current = null;
    }
  }, []);

  /** Called when the pointer enters any part of the notch. */
  const onPointerEnter = useCallback(() => {
    clearCollapseTimer();
    if (!expanded) setExpanded(true);
  }, [clearCollapseTimer, expanded, setExpanded]);

  /** Called when the pointer leaves the whole notch. */
  const onPointerLeave = useCallback(() => {
    // A pinned notch stays open regardless of the pointer.
    if (pinned) return;
    // A leave that coincides with a resize is the window moving, not the user
    // walking away. See `ignoreLeaveUntil`.
    if (Date.now() < ignoreLeaveUntil.current) return;
    clearCollapseTimer();
    collapseTimer.current = window.setTimeout(() => {
      collapseTimer.current = null;
      // Re-check the pin at fire time: the user may have pinned the notch
      // during the delay, and the captured value would be stale.
      if (!useNotchStore.getState().pinned) {
        useNotchStore.getState().collapse();
      }
    }, COLLAPSE_DELAY_MS);
  }, [clearCollapseTimer, pinned]);

  /** Toggles the pin. Clicking the compact bar expands and pins in one step. */
  const togglePin = useCallback(() => {
    const state = useNotchStore.getState();
    if (state.pinned) {
      state.setPinned(false);
      state.collapse();
    } else {
      clearCollapseTimer();
      state.setExpanded(true);
      state.setPinned(true);
    }
  }, [clearCollapseTimer]);

  /** Escape always dismisses the notch, pinned or not. */
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        clearCollapseTimer();
        useNotchStore.getState().collapse();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [clearCollapseTimer]);

  // Resize the native window whenever the state changes. The backend owns the
  // dimensions so the compact and expanded sizes cannot drift between the two
  // sides.
  useEffect(() => {
    // The window is about to move and resize; ignore the pointer churn that
    // causes. The window covers this before the backend replies.
    ignoreLeaveUntil.current = Date.now() + 700;
    void windowApi
      .setNotchState(expanded ? "expanded" : "compact")
      .catch((error) => {
        console.error("could not resize the notch:", error);
        // Surface the failure in the bar itself: the notch has no visible
        // console, so a silent rejection looks identical to "nothing happened".
        useNotchStore.getState().setError(String(error));
      });
  }, [expanded]);

  // Clear a pending collapse on unmount so it cannot fire into a dead tree.
  useEffect(() => clearCollapseTimer, [clearCollapseTimer]);

  return { expanded, pinned, onPointerEnter, onPointerLeave, togglePin };
}
