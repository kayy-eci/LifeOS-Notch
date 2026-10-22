import { useMemo } from "react";
import { Play, Pause, SkipBack, SkipForward, Music2 } from "lucide-react";
import { useMediaStore } from "../../stores/useMediaStore";
import { useLyricsStore, findActiveLineIndex } from "../../stores/useLyricsStore";
import { formatDuration } from "../../utils/format";
import { IconButton } from "../../components/ui/IconButton";
import { EmptyState } from "../../components/ui/EmptyState";

/**
 * The music panel.
 *
 * Layout follows the content: a wide artwork-and-controls row at the top, then
 * the lyrics. When lyrics are available the panel splits into two columns —
 * the player stays on the left so the controls do not move when the lyrics
 * appear.
 */
export function MusicPanel({ showLyrics }: { showLyrics: boolean }) {
  const session = useMediaStore((s) => s.session);
  const position = useMediaStore((s) => s.position);
  const command = useMediaStore((s) => s.command);
  const seek = useMediaStore((s) => s.seek);

  const lyrics = useLyricsStore((s) => s.lyrics);
  const loadingLyrics = useLyricsStore((s) => s.loading);

  // The backend reports the authoritative position; this keeps the bar moving
  // between pushes without ever disagreeing with it for long.
  const duration = session?.duration_seconds ?? null;
  const activeIndex = useMemo(() => {
    if (!lyrics?.synced) return -1;
    return findActiveLineIndex(lyrics.lines, Math.round(position * 1000));
  }, [lyrics, position]);

  if (!session) {
    return (
      <EmptyState
        icon={<Music2 size={22} />}
        title="Nothing is playing"
        hint="Start a track in any player that shows up in the Windows media overlay — Spotify, a browser, or the built-in player."
      />
    );
  }

  const isPlaying = session.status === "playing";
  const title = session.title ?? "Untitled track";
  const artist = session.artist ?? session.app_id;

  return (
    <div className="flex h-full min-h-0 gap-3">
      {/* Player column */}
      <div className={`flex min-w-0 flex-col gap-3 ${showLyrics ? "w-64 shrink-0" : "w-full"}`}>
        <div className="flex items-center gap-3">
          {session.artwork ? (
            <img
              src={session.artwork}
              alt=""
              className="size-16 shrink-0 rounded-control object-cover"
            />
          ) : (
            <div className="flex size-16 shrink-0 items-center justify-center rounded-control bg-surface-raised text-text-subtle">
              <Music2 size={20} aria-hidden />
            </div>
          )}

          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-medium text-text" title={title}>
              {title}
            </p>
            <p className="truncate text-xs text-text-muted" title={artist}>
              {artist}
            </p>
            {session.album ? (
              <p className="truncate text-2xs text-text-subtle">{session.album}</p>
            ) : null}
          </div>
        </div>

        {/* Progress. Only rendered when the player reports a duration —
            a fabricated bar would be worse than none. */}
        {duration !== null ? (
          <div className="flex items-center gap-2">
            <span className="tabular w-9 shrink-0 text-right text-2xs text-text-subtle">
              {formatDuration(position)}
            </span>
            <input
              type="range"
              min={0}
              max={Math.max(1, Math.floor(duration))}
              value={Math.min(Math.floor(position), Math.floor(duration))}
              disabled={!session.can_seek}
              aria-label="Seek"
              onChange={(e) => void seek(Number(e.target.value))}
              className="h-1 w-full cursor-pointer appearance-none rounded-pill bg-track
                accent-[var(--color-accent)] disabled:cursor-default disabled:opacity-50"
            />
            <span className="tabular w-9 shrink-0 text-2xs text-text-subtle">
              {formatDuration(duration)}
            </span>
          </div>
        ) : (
          <p className="text-2xs text-text-subtle">This player does not report a track length.</p>
        )}

        <div className="flex items-center justify-center gap-1">
          <IconButton
            label="Previous track"
            disabled={!session.can_skip_previous}
            onClick={() => void command("previous")}
          >
            <SkipBack size={16} aria-hidden />
          </IconButton>

          <IconButton
            label={isPlaying ? "Pause" : "Play"}
            size="md"
            disabled={isPlaying ? !session.can_pause : !session.can_play}
            onClick={() => void command("toggle")}
            className="bg-surface-raised"
          >
            {isPlaying ? <Pause size={16} aria-hidden /> : <Play size={16} aria-hidden />}
          </IconButton>

          <IconButton
            label="Next track"
            disabled={!session.can_skip_next}
            onClick={() => void command("next")}
          >
            <SkipForward size={16} aria-hidden />
          </IconButton>
        </div>
      </div>

      {/* Lyrics column */}
      {showLyrics ? (
        <div className="scroll-thin min-w-0 flex-1 overflow-y-auto border-l border-border pl-3">
          <LyricsView
            loading={loadingLyrics}
            synced={lyrics?.synced ?? false}
            lines={lyrics?.lines ?? []}
            activeIndex={activeIndex}
            onSeek={(ms) => void seek(ms / 1000)}
            canSeek={session.can_seek}
          />
        </div>
      ) : null}
    </div>
  );
}

interface LyricsViewProps {
  loading: boolean;
  synced: boolean;
  lines: { time_ms: number; text: string }[];
  activeIndex: number;
  onSeek: (timeMs: number) => void;
  canSeek: boolean;
}

function LyricsView({ loading, synced, lines, activeIndex, onSeek, canSeek }: LyricsViewProps) {
  if (loading) {
    return <p className="py-6 text-center text-xs text-text-subtle">Looking up lyrics…</p>;
  }

  if (lines.length === 0) {
    return (
      <EmptyState
        title="No lyrics for this track"
        hint="LRCLIB has nothing for it yet. Newer and local tracks are often missing."
      />
    );
  }

  // Untimed lyrics are shown as plain text and explicitly not called
  // "synchronised" — the feature does not exist for this track.
  if (!synced) {
    return (
      <div className="py-1">
        <p className="mb-2 text-2xs text-text-subtle">
          These lyrics have no timestamps, so they cannot follow playback.
        </p>
        {lines.map((line, i) => (
          <p key={i} className="py-0.5 text-xs leading-relaxed text-text-muted">
            {line.text}
          </p>
        ))}
      </div>
    );
  }

  return (
    <div className="py-1">
      {lines.map((line, i) => {
        const active = i === activeIndex;
        return (
          <button
            key={i}
            type="button"
            disabled={!canSeek}
            onClick={() => onSeek(line.time_ms)}
            className={`block w-full py-0.5 text-left text-xs leading-relaxed transition-colors duration-150
              ${active ? "font-medium text-accent" : "text-text-subtle hover:text-text-muted"}
              disabled:cursor-default`}
          >
            {line.text || " "}
          </button>
        );
      })}
    </div>
  );
}
