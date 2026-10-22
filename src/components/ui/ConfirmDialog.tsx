import { useEffect } from "react";import { AlertTriangle } from "lucide-react";
import { Button } from "./Button";

interface ConfirmDialogProps {
  open: boolean;
  title: string;
  /** What will happen, stated plainly. */
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  /** Renders the confirm button in the danger style. */
  destructive?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

/**
 * A confirmation prompt for destructive actions. Focus moves to the dialog
 * on open and Escape cancels it.
 */
export function ConfirmDialog({
  open,
  title,
  message,
  confirmLabel = "Confirm",
  cancelLabel = "Cancel",
  destructive = false,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  useEffect(() => {
    if (!open) return;
    // Focus the confirm button on open so Escape and Tab work without the
    // user reaching for the mouse. Done by query rather than a ref because
    // `Button` is a plain function component and does not forward refs.
    const dialog = document.getElementById("confirm-title");
    dialog?.closest("[role='dialog']")?.querySelector<HTMLButtonElement>("button[data-confirm]")?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, onCancel]);

  if (!open) return null;

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="confirm-title"
      aria-describedby="confirm-message"
      className="absolute inset-0 z-20 flex items-center justify-center bg-black/50 p-4"
    >
      <div className="w-full max-w-72 animate-fade-in rounded-surface border border-border-strong bg-surface-raised p-3 shadow-pop">
        <div className="flex items-start gap-2">
          <AlertTriangle
            size={14}
            className={`mt-0.5 shrink-0 ${destructive ? "text-danger" : "text-warning"}`}
            aria-hidden
          />
          <div className="min-w-0">
            <h2 id="confirm-title" className="text-sm font-medium text-text">
              {title}
            </h2>
            <p id="confirm-message" className="mt-1 text-xs leading-snug text-text-muted">
              {message}
            </p>
          </div>
        </div>
        <div className="mt-3 flex justify-end gap-2">
          <Button size="sm" variant="ghost" onClick={onCancel}>
            {cancelLabel}
          </Button>
          <Button
            data-confirm=""
            size="sm"
            variant={destructive ? "danger" : "primary"}
            onClick={onConfirm}
          >
            {confirmLabel}
          </Button>
        </div>
      </div>
    </div>
  );
}
