/**
 * The screen of the download queue.
 *
 * The queue is the memory of the launcher about the files it fetched.
 * An installation that was interrupted leaves a trace of it, and the
 * player can come here to see what failed and to fetch it again.
 *
 * The screen never guesses: everything it shows is what the backend
 * wrote into the database, and every button it offers is a command that
 * exists on the other side of the IPC.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo } from "react";

import { Button } from "../components/Button";
import { EmptyState, ErrorState, LoadingState } from "../components/States";
import {
  clearCompletedDownloads,
  clearFailedDownloads,
  listDownloads,
  retryDownload,
  toCommandError,
} from "../ipc/client";
import { queryKeys } from "../ipc/queries";
import type { DownloadEntry, DownloadState } from "../ipc/types";
import { formatBytes } from "../lib/format";

/** The screen of the download queue. */
export function DownloadsRoute() {
  const queryClient = useQueryClient();

  const downloads = useQuery({
    queryKey: queryKeys.downloads(),
    queryFn: listDownloads,
  });

  // Every command that changes the queue refreshes it, so that the
  // table never shows what the backend has already forgotten.
  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: queryKeys.downloads() });
  };

  const clearCompleted = useMutation({
    mutationFn: clearCompletedDownloads,
    onSuccess: refresh,
  });

  const clearFailed = useMutation({
    mutationFn: clearFailedDownloads,
    onSuccess: refresh,
  });

  const retry = useMutation({
    mutationFn: retryDownload,
    onSuccess: refresh,
  });

  const entries = useMemo(() => downloads.data ?? [], [downloads.data]);
  const failed = entries.filter((entry) => entry.state === "failed");
  const completed = entries.filter((entry) => entry.state === "completed");

  if (downloads.isPending) {
    return <LoadingState label="Читаем очередь загрузок…" />;
  }

  if (downloads.isError) {
    return <ErrorState error={downloads.error} />;
  }

  return (
    <div className="screen">
      <div className="screen__header">
        <div>
          <h1 className="screen__title">Загрузки</h1>
          <p className="screen__subtitle">
            {entries.length} записей, из них неудачных — {failed.length}
          </p>
        </div>
        <div className="screen__actions">
          <Button
            busy={clearFailed.isPending}
            disabled={failed.length === 0}
            onClick={() => clearFailed.mutate()}
          >
            Очистить неудачные
          </Button>
          <Button
            busy={clearCompleted.isPending}
            disabled={completed.length === 0}
            onClick={() => clearCompleted.mutate()}
          >
            Очистить завершённые
          </Button>
        </div>
      </div>

      {clearFailed.isError ? (
        <p className="field__error">{toCommandError(clearFailed.error).message}</p>
      ) : null}

      {clearCompleted.isError ? (
        <p className="field__error">{toCommandError(clearCompleted.error).message}</p>
      ) : null}

      {retry.isError ? <p className="field__error">{toCommandError(retry.error).message}</p> : null}

      {entries.length === 0 ? (
        <EmptyState
          title="Очередь пуста"
          description="Загрузки появятся здесь, когда лаунчер начнёт устанавливать версию."
        />
      ) : (
        <div className="card" style={{ padding: 0 }}>
          <table className="table">
            <thead>
              <tr>
                <th>Файл</th>
                <th>Состояние</th>
                <th>Размер</th>
                <th>Попытки</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {entries.map((entry) => (
                <tr key={entry.url}>
                  <td className="mono selectable">{nameOf(entry)}</td>
                  <td>
                    <span className={badgeOf(entry.state)}>{STATE_LABELS[entry.state]}</span>
                  </td>
                  <td className="mono">{formatBytes(entry.bytes_done)}</td>
                  <td className="mono">{entry.attempts}</td>
                  <td>
                    <div className="table__actions">
                      {entry.state === "failed" ? (
                        <Button
                          variant="primary"
                          busy={retry.isPending && retry.variables === entry.url}
                          onClick={() => retry.mutate(entry.url)}
                        >
                          Повторить
                        </Button>
                      ) : null}
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <section className="card">
        <div className="card__title">Что такое очередь</div>
        <p className="field__hint">
          Каждая запись — один файл, который лаунчер когда-то качал. Неудачную можно перекачать ещё
          раз: файл пишется рядом с нужным местом и переносится на место только тогда, когда его
          целиком скачали и сверили с контрольной суммой.
        </p>
      </section>
    </div>
  );
}

/** The name of a file, without the whole path to it. */
function nameOf(entry: DownloadEntry): string {
  const path = entry.target_path.replaceAll("\\", "/");
  const parts = path.split("/");
  return parts[parts.length - 1] || entry.target_path;
}

/** The class of the badge of a state. */
function badgeOf(state: DownloadState): string {
  if (state === "completed") {
    return "badge badge--success";
  }
  if (state === "failed") {
    return "badge badge--warning";
  }
  if (state === "downloading") {
    return "badge badge--accent";
  }
  return "badge";
}

/** The names of the states, in Russian. */
const STATE_LABELS: Record<DownloadState, string> = {
  queued: "В очереди",
  downloading: "Качается",
  completed: "Готово",
  failed: "Ошибка",
};
