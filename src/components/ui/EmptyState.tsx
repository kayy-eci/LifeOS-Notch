import type { ReactNode } from "react";

interface EmptyStateProps {
  /** What is missing, stated plainly. */
  title: string;
  /** What the user can do about it. */
  hint?: string;
  icon?: ReactNode;
  action?: ReactNode;
}

/**
 * Shown when a panel has nothing to display. It always says what is absent
 * and what would fill it — never a decorative placeholder.
 */
export function EmptyState({ title, hint, icon, action }: EmptyStateProps) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 px-6 py-8 text-center">
      {icon ? <div className="text-text-subtle">{icon}</div> : null}
      <p className="text-sm text-text-muted">{title}</p>
      {hint ? <p className="max-w-64 text-xs text-text-subtle">{hint}</p> : null}
      {action ? <div className="pt-1">{action}</div> : null}
    </div>
  );
}
