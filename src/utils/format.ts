/**
 * Formatting helpers shared by the panels.
 *
 * These are pure so they can be unit tested without a DOM.
 */

/** `95` -> `"1:35"`, `3725` -> `"1:02:05"`. */
export function formatDuration(totalSeconds: number): string {
  const seconds = Math.max(0, Math.floor(totalSeconds));
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const rest = seconds % 60;
  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${String(rest).padStart(2, "0")}`;
  }
  return `${minutes}:${String(rest).padStart(2, "0")}`;
}

/** Millilitres as a human amount: `1500` -> `"1.5 L"`, `250` -> `"250 ml"`. */
export function formatVolume(ml: number): string {
  if (ml >= 1000) {
    const litres = ml / 1000;
    // One decimal below 10 L so the readout stays short.
    return `${litres < 10 ? litres.toFixed(1) : Math.round(litres)} L`;
  }
  return `${Math.round(ml)} ml`;
}

/** Local calendar date as `YYYY-MM-DD`. */
export function localDateKey(date: Date = new Date()): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

/** Clock time as `HH:MM`. */
export function formatClock(date: Date = new Date()): string {
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

/** Date and clock time, e.g. `Thu 15 Oct · 18:55`. */
export function formatDateClock(date: Date = new Date()): string {
  const day = date.toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" });
  return `${day} · ${formatClock(date)}`;
}

/**
 * A due date relative to today, e.g. `Today`, `Tomorrow`, `Fri 24 Oct`.
 * Returns `null` for a missing due date so callers can hide the label.
 */
export function formatDueDate(dueAt: number | null, now: Date = new Date()): string | null {
  if (dueAt === null) return null;
  const due = new Date(dueAt);
  const todayKey = localDateKey(now);
  const dueKey = localDateKey(due);
  if (dueKey === todayKey) return "Today";
  const tomorrow = new Date(now);
  tomorrow.setDate(tomorrow.getDate() + 1);
  if (dueKey === localDateKey(tomorrow)) return "Tomorrow";
  const sameYear = due.getFullYear() === now.getFullYear();
  return due.toLocaleDateString(undefined, {
    weekday: "short",
    day: "numeric",
    month: "short",
    ...(sameYear ? {} : { year: "numeric" }),
  });
}
