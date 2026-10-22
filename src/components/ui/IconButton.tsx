import type { ButtonHTMLAttributes, ReactNode } from "react";

interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** Required: an icon-only control with no visible text needs an accessible name. */
  label: string;
  children: ReactNode;
  active?: boolean;
  size?: "sm" | "md";
}

const SIZES = {
  sm: "size-6 rounded-control",
  md: "size-8 rounded-control",
} as const;

/**
 * An icon-only button. `label` becomes the accessible name, and is also used
 * as the native tooltip so the control is discoverable without a hover
 * component.
 */
export function IconButton({
  label,
  children,
  active = false,
  size = "md",
  className = "",
  ...rest
}: IconButtonProps) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      aria-pressed={active}
      className={`inline-flex shrink-0 items-center justify-center
        transition-colors duration-100 disabled:cursor-not-allowed disabled:opacity-40
        ${
          active
            ? "bg-accent-muted text-accent"
            : "text-text-muted hover:bg-surface-hover hover:text-text"
        }
        ${SIZES[size]} ${className}`}
      {...rest}
    >
      {children}
    </button>
  );
}
