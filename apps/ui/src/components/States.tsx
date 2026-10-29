/** Empty, loading and error states shared by the screens. */

import type { ReactNode } from "react";

import { toCommandError } from "../ipc/client";

export interface EmptyStateProps {
  title: string;
  description?: string;
  action?: ReactNode;
}

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
  label?: string;
}

export function LoadingState({ label = "Загрузка…" }: LoadingStateProps) {
  return (
    <div className="empty empty--loading" role="status" aria-live="polite">
      <span className="spinner" aria-hidden="true" />
      <div className="empty__description">{label}</div>
    </div>
  );
}

export interface ErrorStateProps {
  error: unknown;
  action?: ReactNode;
}

/** The backend message is shown as written; the detail sits below it. */
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
