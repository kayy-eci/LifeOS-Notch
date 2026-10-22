import { useEffect, useState } from "react";
import { Play, Pause, SkipForward, RotateCcw, Timer } from "lucide-react";
import { usePomodoroStore } from "../../stores/usePomodoroStore";
import { useSettingsStore } from "../../stores/useSettingsStore";
import { useCountdown } from "../../hooks/useClock";
import { formatDuration } from "../../utils/format";
import { Button } from "../../components/ui/Button";
import { IconButton } from "../../components/ui/IconButton";
import { ProgressRing } from "../../components/ui/ProgressRing";
import { ErrorNotice } from "../../components/ui/ErrorNotice";

const MODE_LABEL = {
  focus: "Focus session",
  short_break: "Short break",
  long_break: "Long break",
} as const;

/**
 * The focus timer.
 *
 * The ring shows elapsed progress through the current phase. The remaining
 * time is derived from the backend's deadline, so it stays correct while the
 * panel is closed — the panel is a view of the timer, not the timer itself.
 */
export function PomodoroPanel() {
  const state = usePomodoroStore((s) => s.state);
  const error = usePomodoroStore((s) => s.error);
  const start = usePomodoroStore((s) => s.start);
  const pause = usePomodoroStore((s) => s.pause);
  const skip = usePomodoroStore((s) => s.skip);
  const reset = usePomodoroStore((s) => s.reset);
  const load = usePomodoroStore((s) => s.load);

  const settings = useSettingsStore((s) => s.settings);

  // A local ticker only drives the display; the deadline is the source of
  // truth and comes from the backend.
  const remaining = useCountdown(state?.ends_at ?? null, state?.status === "running");

  // Re-render while paused too, so a resumed session shows the right value.
  const [, tick] = useState(0);
  useEffect(() => {
    if (state?.status !== "paused") return;
    const id = window.setInterval(() => tick((n) => n + 1), 1000);
    return () => window.clearInterval(id);
  }, [state?.status]);

  useEffect(() => {
    if (!state) void load();
  }, [state, load]);

  if (!state) {
    return <p className="py-8 text-center text-xs text-text-subtle">Loading the timer…</p>;
  }

  const modeSeconds = (() => {
    if (!settings) return 25 * 60;
    switch (state.mode) {
      case "focus":
        return settings.focus_minutes * 60;
      case "short_break":
        return settings.short_break_minutes * 60;
      case "long_break":
        return settings.long_break_minutes * 60;
    }
  })();

  const secondsLeft =
    state.status === "paused" ? (state.remaining_seconds ?? 0) : state.status === "running" ? remaining : modeSeconds;

  const progress = modeSeconds > 0 ? 1 - secondsLeft / modeSeconds : 0;
  const running = state.status === "running";
  const paused = state.status === "paused";

  return (
    <div className="flex flex-col items-center gap-3">
      {error ? <ErrorNotice message={error} className="w-full" onRetry={() => void load()} /> : null}

      <ProgressRing
        value={progress}
        label={`${MODE_LABEL[state.mode]} progress`}
        size={148}
        strokeWidth={7}
      >
        <span className="tabular text-2xl font-medium text-text">{formatDuration(secondsLeft)}</span>
        <span className="mt-0.5 text-2xs text-text-subtle">{MODE_LABEL[state.mode]}</span>
      </ProgressRing>

      <p className="text-xs text-text-muted">
        {running
          ? state.mode === "focus"
            ? "Focus session running"
            : "Break running"
          : paused
            ? "Paused"
            : "Ready to start"}
      </p>

      <div className="flex items-center gap-1.5">
        <Button
          size="sm"
          variant="primary"
          onClick={() => void (running ? pause() : start())}
        >
          {running ? <Pause size={12} aria-hidden /> : <Play size={12} aria-hidden />}
          {running ? "Pause" : paused ? "Resume" : "Start focus"}
        </Button>

        <IconButton label="Skip to the next phase" onClick={() => void skip()}>
          <SkipForward size={14} aria-hidden />
        </IconButton>

        <IconButton label="Reset the timer" onClick={() => void reset()}>
          <RotateCcw size={14} aria-hidden />
        </IconButton>
      </div>

      <div className="mt-1 flex w-full items-center justify-center gap-4 border-t border-border pt-2 text-2xs text-text-subtle">
        <span className="flex items-center gap-1">
          <Timer size={11} aria-hidden />
          {state.completed_today} today
        </span>
        <span>{state.completed_total} total</span>
        <span>
          {state.completed_focus_sessions} of {settings?.long_break_after ?? 4} to a long break
        </span>
      </div>
    </div>
  );
}
