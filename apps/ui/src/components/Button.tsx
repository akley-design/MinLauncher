/** The shared button; a busy button shows its own spinner. */

import type { ButtonHTMLAttributes, ReactNode } from "react";

export type ButtonVariant = "default" | "primary" | "danger" | "ghost";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  busy?: boolean;
  children: ReactNode;
}

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
