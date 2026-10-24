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
    <div className="empty empty--empty" role="status" aria-live="polite">
      <span className="empty__icon" aria-hidden="true">
        <svg viewBox="0 0 40 40">
          <path d="M7 13.5 20 7l13 6.5v13L20 33 7 26.5z" />
          <path d="m7.5 13.5 12.5 6.2 12.5-6.2M20 20v12.5" />
          <path d="M13 10.5 26 17" />
        </svg>
      </span>
      <div className="empty__copy">
        <div className="empty__title">{title}</div>
        {description ? <div className="empty__description">{description}</div> : null}
      </div>
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
    <div className="empty empty--loading" role="status" aria-live="polite">
      <span className="spinner" aria-hidden="true" />
      <div className="empty__description">{label}</div>
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
    <div className="empty empty--error" role="alert" aria-live="polite">
      <span className="empty__icon" aria-hidden="true">
        !
      </span>
      <div className="empty__copy">
        <div className="empty__title">Что-то пошло не так</div>
        <div className="empty__description selectable">{commandError.message}</div>
      </div>
      {commandError.detail ? (
        <div className="empty__detail selectable">{commandError.detail}</div>
      ) : null}
      {action}
    </div>
  );
}
