import type { ButtonHTMLAttributes, ReactNode } from "react";

type Variant = "primary" | "secondary" | "ghost" | "danger";
type Size = "sm" | "md";

const VARIANTS: Record<Variant, string> = {
  primary:
    "bg-accent text-accent-contrast hover:bg-accent-strong active:bg-accent disabled:bg-accent/40",
  secondary:
    "bg-surface-raised text-text border border-border hover:bg-surface-hover hover:border-border-strong",
  ghost: "text-text-muted hover:bg-surface-raised hover:text-text",
  danger: "bg-danger-muted text-danger border border-danger/30 hover:bg-danger/20",
};

const SIZES: Record<Size, string> = {
  sm: "h-7 px-2.5 text-xs gap-1.5",
  md: "h-8 px-3 text-sm gap-2",
};

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: Size;
  children: ReactNode;
}

/**
 * The standard action button. Square-ish corners and a 1px border on the
 * secondary variant keep the control reading as a native Windows tool button
 * rather than a web call-to-action.
 */
export function Button({
  variant = "secondary",
  size = "md",
  className = "",
  children,
  ...rest
}: ButtonProps) {
  return (
    <button
      type="button"
      className={`inline-flex items-center justify-center rounded-control font-medium
        transition-colors duration-100 disabled:cursor-not-allowed disabled:opacity-50
        ${VARIANTS[variant]} ${SIZES[size]} ${className}`}
      {...rest}
    >
      {children}
    </button>
  );
}
