import { useEffect, useRef, useState } from "react";

/**
 * A clock that re-renders on an interval, but only while it is needed.
 *
 * Used for the compact notch's time display and for the pomodoro countdown.
 * The interval is aligned to the next whole second so the displayed time does
 * not visibly lag behind the real clock by up to a second.
 */
export function useClock(active = true, intervalMs = 1000): Date {
  const [now, setNow] = useState(() => new Date());

  useEffect(() => {
    if (!active) return;

    let timer: number;
    let cancelled = false;

    const tick = () => {
      if (cancelled) return;
      setNow(new Date());
      // Align to the next second boundary so the digits change on the beat.
      const delay = intervalMs - (Date.now() % intervalMs);
      timer = window.setTimeout(tick, delay);
    };

    const delay = intervalMs - (Date.now() % intervalMs);
    timer = window.setTimeout(tick, delay);

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [active, intervalMs]);

  return now;
}

/**
 * Counts down to a deadline.
 *
 * Deliberately derived from `Date.now()` rather than decrementing a counter:
 * the remaining time is always correct, even if a tick is delayed by a busy
 * frame or the machine sleeps. Returns `0` once the deadline has passed.
 */
export function useCountdown(endsAt: number | null, active = true): number {
  const [, forceRender] = useState(0);
  const endsAtRef = useRef(endsAt);
  endsAtRef.current = endsAt;

  useEffect(() => {
    if (!active || endsAt === null) return;
    const timer = window.setInterval(() => forceRender((n) => n + 1), 250);
    return () => window.clearInterval(timer);
  }, [active, endsAt]);

  if (endsAt === null) return 0;
  return Math.max(0, Math.ceil((endsAt - Date.now()) / 1000));
}
