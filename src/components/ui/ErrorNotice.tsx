import { AlertCircle, RotateCw } from "lucide-react";
import { Button } from "./Button";

interface ErrorNoticeProps {
  /** What failed, in the user's terms. */
  message: string;
  /** What to try next, when there is a single obvious action. */
  hint?: string;
  onRetry?: () => void;
  retryLabel?: string;
  className?: string;
}

/**
 * A failure the user can act on. The message always says what happened and
 * the hint says what to do; the retry action is only rendered when a retry
 * actually exists.
 */
export function ErrorNotice({
  message,
  hint,
  onRetry,
  retryLabel = "Retry",
  className = "",
}: ErrorNoticeProps) {
  return (
    <div
      role="alert"
      className={`flex items-start gap-2 rounded-control border border-danger/30
        bg-danger-muted px-2.5 py-2 ${className}`}
    >
      <AlertCircle size={14} className="mt-0.5 shrink-0 text-danger" aria-hidden />
      <div className="min-w-0 flex-1">
        <p className="text-xs leading-snug text-text">{message}</p>
        {hint ? <p className="mt-0.5 text-xs leading-snug text-text-subtle">{hint}</p> : null}
      </div>
      {onRetry ? (
        <Button size="sm" variant="ghost" onClick={onRetry} className="shrink-0">
          <RotateCw size={12} aria-hidden />
          {retryLabel}
        </Button>
      ) : null}
    </div>
  );
}
