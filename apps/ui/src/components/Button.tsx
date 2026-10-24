/**
 * The button of the launcher.
 *
 * A thin wrapper over `<button>`: it exists so that every button of the
 * interface looks the same, and so that a button that is busy says so
 * without every caller having to invent a spinner.
 */

import type { ButtonHTMLAttributes, ReactNode } from "react";

export type ButtonVariant = "default" | "primary" | "danger" | "ghost";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** The look of the button. */
  variant?: ButtonVariant;
  /** Whether the button is doing something and cannot be pressed again. */
  busy?: boolean;
  /** The content of the button. */
  children: ReactNode;
}

/** A button of the launcher. */
export function Button({
  variant = "default",
  busy = false,
  children,
  className,
  disabled,
  ...rest
}: ButtonProps) {
  const classes = ["button"];
  if (variant !== "default") {
    classes.push(`button--${variant}`);
  }
  if (className) {
    classes.push(className);
  }

  return (
    <button className={classes.join(" ")} disabled={disabled || busy} aria-busy={busy} {...rest}>
      {busy ? <span className="spinner" aria-hidden="true" /> : null}
      {children}
    </button>
  );
}
