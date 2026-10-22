import { useId } from "react";
import type { InputHTMLAttributes, ReactNode, SelectHTMLAttributes } from "react";

interface FieldProps {
  label: string;
  hint?: string;
  error?: string | null;
  children: (id: string) => ReactNode;
}

/**
 * A labelled control. The label is a real `<label>` bound to the control id
 * so screen readers announce it, and the hint/error text is wired through
 * `aria-describedby`.
 */
export function Field({ label, hint, error, children }: FieldProps) {
  const id = useId();
  const hintId = `${id}-hint`;
  const errorId = `${id}-error`;
  const describedBy = [hint ? hintId : null, error ? errorId : null].filter(Boolean).join(" ");

  return (
    <div className="flex flex-col gap-1">
      <label htmlFor={id} className="text-xs text-text-muted">
        {label}
      </label>
      {children(id)}
      {hint ? (
        <p id={hintId} className="text-2xs text-text-subtle">
          {hint}
        </p>
      ) : null}
      {error ? (
        <p id={errorId} className="text-2xs text-danger">
          {error}
        </p>
      ) : null}
      <span className="sr-only">{describedBy ? "" : null}</span>
    </div>
  );
}

const controlClass =
  "h-8 w-full rounded-control border border-border bg-surface-sunken px-2 text-sm text-text " +
  "placeholder:text-text-subtle transition-colors duration-100 " +
  "hover:border-border-strong focus:border-accent focus:outline-none";

interface TextInputProps extends InputHTMLAttributes<HTMLInputElement> {
  label: string;
  hint?: string;
  error?: string | null;
}

export function TextInput({ label, hint, error, className = "", ...rest }: TextInputProps) {
  return (
    <Field label={label} hint={hint} error={error}>
      {(id) => (
        <input
          id={id}
          className={`${controlClass} ${className}`}
          aria-invalid={error ? true : undefined}
          {...rest}
        />
      )}
    </Field>
  );
}

interface SelectProps extends SelectHTMLAttributes<HTMLSelectElement> {
  label: string;
  hint?: string;
  options: { value: string; label: string }[];
}

export function Select({ label, hint, options, className = "", ...rest }: SelectProps) {
  return (
    <Field label={label} hint={hint}>
      {(id) => (
        <select id={id} className={`${controlClass} ${className}`} {...rest}>
          {options.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
      )}
    </Field>
  );
}

interface ToggleProps {
  label: string;
  hint?: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
}

/** A switch. Uses a checkbox so it is keyboard accessible for free. */
export function Toggle({ label, hint, checked, onChange, disabled }: ToggleProps) {
  const id = useId();
  return (
    <div className="flex items-start justify-between gap-3">
      <label htmlFor={id} className="min-w-0 cursor-pointer">
        <span className="block text-xs text-text">{label}</span>
        {hint ? <span className="mt-0.5 block text-2xs text-text-subtle">{hint}</span> : null}
      </label>
      <button
        id={id}
        type="button"
        role="switch"
        aria-checked={checked}
        disabled={disabled}
        onClick={() => onChange(!checked)}
        className={`relative mt-0.5 h-4.5 w-8 shrink-0 rounded-pill border transition-colors duration-150
          ${
            checked
              ? "border-accent bg-accent"
              : "border-border-strong bg-surface-sunken hover:border-text-subtle"
          } disabled:opacity-50`}
      >
        <span
          className={`absolute top-0.5 size-3.5 rounded-pill bg-text transition-transform duration-150
            ${checked ? "translate-x-3.5 bg-accent-contrast" : "translate-x-0.5"}`}
        />
      </button>
    </div>
  );
}
