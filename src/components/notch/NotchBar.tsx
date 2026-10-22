import { Pin, PinOff, Music2, Timer, Droplets } from "lucide-react";
import { IconButton } from "../ui/IconButton";
import { useNotchStore } from "../../stores/useNotchStore";

interface NotchBarProps {
  expanded: boolean;
  clock: string;
  track: string | null;
  artist: string | null;
  artwork: string | null;
  isPlaying: boolean;
  focusLabel: string | null;
  waterTotal: number;
  waterTarget: number;
  onTogglePin: () => void;
}

/**
 * The always-visible compact bar.
 *
 * It is the only thing on screen most of the time, so it carries three pieces
 * of information and nothing else: the time, what is playing, and any running
 * focus session. A badge only appears when its feature is actually active —
 * an idle pomodoro shows nothing rather than a zero.
 */
export function NotchBar({
  expanded,
  clock,
  track,
  artist,
  artwork,
  isPlaying,
  focusLabel,
  waterTotal,
  waterTarget,
  onTogglePin,
}: NotchBarProps) {
  const waterPct = waterTarget > 0 ? Math.min(1, waterTotal / waterTarget) : 0;
  const error = useNotchStore((s) => s.error);

  return (
    <div
      className={`relative flex h-11 shrink-0 items-center gap-3 bg-surface px-3.5
        transition-[border-radius,box-shadow] duration-200
        ${expanded ? "rounded-t-notch rounded-b-none shadow-panel" : "rounded-b-notch shadow-notch"}`}
    >
      {/* Clock: the anchor of the compact state. */}
      <span className="tabular shrink-0 text-base font-medium text-text">{clock}</span>

      <div className="h-4 w-px shrink-0 bg-border" aria-hidden />

      {/* Now playing, or an honest statement that nothing is. */}
      <div className="flex min-w-0 flex-1 items-center gap-2">
        {artwork ? (
          <img
            src={artwork}
            alt=""
            className={`shrink-0 rounded-[3px] object-cover transition-all duration-200
              ${isPlaying ? "size-5" : "size-5 opacity-60 grayscale"}`}
          />
        ) : (
          <Music2 size={13} className="shrink-0 text-text-subtle" aria-hidden />
        )}
        {track ? (
          <p className="min-w-0 truncate text-xs text-text-muted">
            <span className="text-text">{track}</span>
            {artist ? <span className="text-text-subtle"> · {artist}</span> : null}
          </p>
        ) : (
          <p className="truncate text-xs text-text-subtle">Nothing playing</p>
        )}
      </div>

      {/* Active-state badges. Each is absent unless its feature is running. */}
      {focusLabel ? (
        <span className="tabular flex shrink-0 items-center gap-1 rounded-control bg-accent-muted px-1.5 py-0.5 text-2xs font-medium text-accent">
          <Timer size={11} aria-hidden />
          {focusLabel}
        </span>
      ) : null}

      {waterTotal > 0 ? (
        <span className="tabular flex shrink-0 items-center gap-1 text-2xs text-text-subtle">
          <Droplets size={11} aria-hidden />
          {Math.round(waterPct * 100)}%
        </span>
      ) : null}

      <IconButton
        label={expanded ? "Close the panel" : "Open the panel"}
        size="sm"
        onClick={onTogglePin}
        active={expanded}
      >
        {expanded ? <PinOff size={13} aria-hidden /> : <Pin size={13} aria-hidden />}
      </IconButton>
    </div>
  );
}
