import { useEffect } from "react";
import { formatClock, formatDateClock } from "./utils/format";
import { useClock } from "./hooks/useClock";
import { useNotchInteraction } from "./hooks/useNotchInteraction";
import { useNotchStore } from "./stores/useNotchStore";
import { useMediaStore } from "./stores/useMediaStore";
import { usePomodoroStore, subscribeToPomodoro } from "./stores/usePomodoroStore";
import { useWaterStore, subscribeToWater } from "./stores/useWaterStore";
import { useNotionStore, subscribeToNotion } from "./stores/useNotionStore";
import { useSettingsStore } from "./stores/useSettingsStore";
import { subscribeToMedia } from "./stores/useMediaStore";
import { NotchBar } from "./components/notch/NotchBar";
import { ExpandedPanel } from "./components/notch/ExpandedPanel";
import { formatDuration } from "./utils/format";

/**
 * The application shell.
 *
 * Everything that changes the notch state funnels through
 * `useNotchInteraction`, so there is one owner of the expand/collapse rules
 * and one place that resizes the native window.
 */
function App() {
  const { expanded, onPointerEnter, onPointerLeave, togglePin } = useNotchInteraction();
  const activePanel = useNotchStore((s) => s.activePanel);
  const setBooting = useNotchStore((s) => s.setBooting);

  const now = useClock(true);

  const media = useMediaStore((s) => s.session);
  const loadMedia = useMediaStore((s) => s.load);
  const pomodoro = usePomodoroStore((s) => s.state);
  const loadPomodoro = usePomodoroStore((s) => s.load);
  const water = useWaterStore((s) => s.day);
  const loadWater = useWaterStore((s) => s.load);
  const loadSettings = useSettingsStore((s) => s.load);
  const loadNotion = useNotionStore((s) => s.load);

  // One-time bootstrap: read the initial state from the backend and subscribe
  // to the pushes it emits. The backend owns the timer and the media session,
  // so the UI never polls.
  useEffect(() => {
    let cancelled = false;

    void (async () => {
      await Promise.all([loadSettings(), loadPomodoro(), loadWater(), loadMedia(), loadNotion()]);
      if (!cancelled) setBooting(false);
    })();

    const unsubscribers = [subscribeToPomodoro(), subscribeToWater(), subscribeToMedia(), subscribeToNotion()];

    return () => {
      cancelled = true;
      for (const unsubscribe of unsubscribers) {
        void unsubscribe.then((fn) => fn());
      }
    };
  }, [loadSettings, loadPomodoro, loadWater, loadMedia, loadNotion, setBooting]);

  const focusLabel =
    pomodoro && pomodoro.status === "running" && pomodoro.ends_at
      ? formatDuration(Math.max(0, Math.ceil((pomodoro.ends_at - Date.now()) / 1000)))
      : null;

  return (
    <div
      // The window is exactly the size of this element, so entering it means
      // the pointer is over the notch. Pointer events are attached here rather
      // than to the bar so the expanded panel keeps them while open.
      className="flex h-screen w-screen flex-col overflow-hidden"
      onPointerEnter={onPointerEnter}
      onPointerLeave={onPointerLeave}
    >
      <NotchBar
        expanded={expanded}
        clock={formatClock(now)}
        track={media?.title ?? null}
        artist={media?.artist ?? null}
        artwork={media?.artwork ?? null}
        isPlaying={media?.status === "playing"}
        focusLabel={focusLabel}
        waterTotal={water?.total_ml ?? 0}
        waterTarget={water?.target_ml ?? 0}
        onTogglePin={togglePin}
      />

      {expanded ? (
        <ExpandedPanel
          activePanel={activePanel}
          dateLabel={formatDateClock(now)}
          onTogglePin={togglePin}
        />
      ) : null}
    </div>
  );
}

export default App;
