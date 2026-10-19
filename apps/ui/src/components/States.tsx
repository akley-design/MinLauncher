/**
 * The states of a screen that is waiting for something.
 *
 * A screen that has nothing to show must say so. A blank panel looks
 * broken, and a spinner that never ends looks worse than either.
 */

import type { ReactNode } from "react";

import { toCommandError } from "../ipc/client";

export interface EmptyStateProps {
  /** The headline of the empty state. */
  title: string;
  /** The explanation under the headline. */
  description?: string;
  /** What the player can do about it. */
  action?: ReactNode;
}

/** A screen that has nothing to show. */
export function EmptyState({ title, description, action }: EmptyStateProps) {
  return (
    <div className="empty">
      <div className="empty__title">{title}</div>
      {description ? <div>{description}</div> : null}
      {action}
    </div>
  );
}

export interface LoadingStateProps {
  /** What is being waited for. */
  label?: string;
}

/** A screen that is waiting. */
export function LoadingState({ label = "Загрузка…" }: LoadingStateProps) {
  return (
    <div className="empty">
      <span className="spinner" aria-hidden="true" />
      <div>{label}</div>
    </div>
  );
}

export interface ErrorStateProps {
  /** The failure that happened. */
  error: unknown;
  /** What the player can do about it. */
  action?: ReactNode;
}

/**
 * A screen that failed.
 *
 * The message of the backend is shown as it is: it was written for a
 * player, not for a log. The cause is shown under it, in a smaller
 * font, because it helps and because it is not the point.
 *
 * The failure can arrive in several shapes — the object the backend
 * sends, an `Error` of the bridge, or something else entirely — and
 * they are all turned into the same one here, so that the player never
 * sees `[object Object]`.
 */
export function ErrorState({ error, action }: ErrorStateProps) {
  const commandError = toCommandError(error);

  return (
    <div className="empty">
      <div className="empty__title">Что-то пошло не так</div>
      <div className="selectable">{commandError.message}</div>
      {commandError.detail ? (
        <div
          className="selectable"
          style={{ color: "var(--text-dim)", fontSize: "var(--text-xs)" }}
        >
          {commandError.detail}
        </div>
      ) : null}
      {action}
    </div>
  );
}
