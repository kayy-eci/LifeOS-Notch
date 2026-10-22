interface ProgressBarProps {
  /** 0..1. Values outside the range are clamped. */
  value: number;
  className?: string;
  label?: string;
}

/** A horizontal meter. The filled portion is the only coloured element. */
export function ProgressBar({ value, className = "", label }: ProgressBarProps) {
  const pct = Math.max(0, Math.min(1, value)) * 100;
  return (
    <div
      role="progressbar"
      aria-valuenow={Math.round(pct)}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-label={label}
      className={`h-1.5 w-full overflow-hidden rounded-pill bg-track ${className}`}
    >
      <div
        className="h-full rounded-pill bg-accent transition-[width] duration-300 ease-out"
        style={{ width: `${pct}%` }}
      />
    </div>
  );
}
