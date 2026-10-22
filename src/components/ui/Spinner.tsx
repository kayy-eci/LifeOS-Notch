import type { ReactNode } from "react";

interface SpinnerProps {
  label?: string;
  className?: string;
}

/** A small indeterminate spinner. The label is announced, not just drawn. */
export function Spinner({ label = "Loading", className = "" }: SpinnerProps) {
  return (
    <span role="status" aria-label={label} className={`inline-flex items-center gap-2 ${className}`}>
      <svg
        className="size-3.5 animate-spin text-text-subtle"
        viewBox="0 0 16 16"
        fill="none"
        aria-hidden
      >
        <circle cx="8" cy="8" r="6.5" stroke="currentColor" strokeOpacity="0.25" strokeWidth="2" />
        <path
          d="M14.5 8A6.5 6.5 0 0 0 8 1.5"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
        />
      </svg>
      <span className="sr-only">{label}</span>
    </span>
  );
}

/** A centred loading row for a panel that has no data yet. */
export function LoadingBlock({ label = "Loading" }: { label?: string }) {
  return (
    <div className="flex items-center justify-center gap-2 px-6 py-10">
      <Spinner label={label} />
      <span className="text-xs text-text-subtle">{label}…</span>
    </div>
  );
}

/** A skeleton line, used where the shape of the content is known. */
export function Skeleton({ className = "" }: { className?: string }) {
  return <div className={`animate-pulse rounded-control bg-surface-raised ${className}`} />;
}

export function InlineSpinner({ children }: { children?: ReactNode }) {
  return (
    <span className="inline-flex items-center gap-1.5 text-xs text-text-subtle">
      <Spinner />
      {children}
    </span>
  );
}
