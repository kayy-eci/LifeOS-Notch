import { create } from "zustand";
import { lyricsApi } from "../services/ipc";
import type { Lyrics, LyricLine } from "../types";

/**
 * Lyrics are keyed by a normalised `artist - title` string so the same track
 * resolves to the same cache entry regardless of the player that reported it.
 */
export function trackKey(artist: string | null, title: string | null): string | null {
  if (!title) return null;
  const clean = (value: string) =>
    value
      .toLowerCase()
      .replace(/\s*[-–—]\s*.*$/, "")
      .replace(/[^a-z0-9]+/g, " ")
      .trim();
  const cleanArtist = clean(artist ?? "");
  const cleanTitle = clean(title);
  if (!cleanTitle) return null;
  return cleanArtist ? `${cleanArtist} - ${cleanTitle}` : cleanTitle;
}

interface LyricsStore {
  lyrics: Lyrics | null;
  loading: boolean;
  error: string | null;
  /** The track the cached lyrics belong to, so a track change can be detected. */
  loadedKey: string | null;
  load: (key: string, title: string, artist: string) => Promise<void>;
  clear: () => void;
}

export const useLyricsStore = create<LyricsStore>((set) => ({
  lyrics: null,
  loading: false,
  error: null,
  loadedKey: null,

  load: async (key, title, artist) => {
    set({ loading: true, error: null });
    try {
      const lyrics = await lyricsApi.get(key, title, artist);
      set({ lyrics, loading: false, loadedKey: key, error: null });
    } catch (cause) {
      set({
        loading: false,
        error: cause instanceof Error ? cause.message : String(cause),
        lyrics: null,
        loadedKey: key,
      });
    }
  },

  clear: () => set({ lyrics: null, loadedKey: null, error: null }),
}));

/**
 * Picks the lyric line that should be highlighted at `positionMs`.
 *
 * Binary search over the timestamped lines: the active line is the last one
 * whose offset is at or before the playhead. Lines without a timestamp (offset
 * `0` on an untimed lyric sheet) are never selected, because pretending an
 * untimed line is synchronised would be a lie about the feature.
 */
export function findActiveLineIndex(lines: LyricLine[], positionMs: number): number {
  if (lines.length === 0) return -1;

  let low = 0;
  let high = lines.length - 1;
  let found = -1;
  while (low <= high) {
    const mid = (low + high) >> 1;
    if (lines[mid].time_ms <= positionMs) {
      found = mid;
      low = mid + 1;
    } else {
      high = mid - 1;
    }
  }
  return found;
}
