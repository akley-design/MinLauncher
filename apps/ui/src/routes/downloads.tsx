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
 *
 * An installation is thousands of files, so the screen shows one row per
 * **task** — the version that is being installed, or the runtime — and
 * the files of the task are behind a click. A player who installed
 * 26.3 sees one row, not four thousand.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Fragment, useMemo, useState } from "react";

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

/** One task of the queue, with the files that belong to it. */
interface TaskGroup {
  /** The name of the task, or an empty string for the entries without one. */
  name: string;
  /** The files of the task. */
  files: DownloadEntry[];
  /** The state of the task: the worst state of its files. */
  state: DownloadState;
  /** How many bytes the task moved. */
  bytes: number;
}

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

  // The tasks that are opened. A task is opened when the player asks for
  // its files, and closed when the player asks again.
  const [opened, setOpened] = useState<Set<string>>(() => new Set());
  const toggle = (name: string) =>
    setOpened((before) => {
      const after = new Set(before);
      if (after.has(name)) {
        after.delete(name);
      } else {
        after.add(name);
      }
      return after;
    });

  const entries = useMemo(() => downloads.data ?? [], [downloads.data]);
  const failed = entries.filter((entry) => entry.state === "failed");
  const completed = entries.filter((entry) => entry.state === "completed");
  const groups = useMemo(() => groupByTask(entries), [entries]);

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
          <div className="screen__eyebrow">Центр загрузок</div>
          <h1 className="screen__title">Загрузки</h1>
          <p className="screen__subtitle">
            {groups.length} задач, {entries.length} записей, из них неудачных — {failed.length}
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
        <p role="alert" aria-live="polite" className="field__error">
          {toCommandError(clearFailed.error).message}
        </p>
      ) : null}

      {clearCompleted.isError ? (
        <p role="alert" aria-live="polite" className="field__error">
          {toCommandError(clearCompleted.error).message}
        </p>
      ) : null}

      {retry.isError ? (
        <p role="alert" aria-live="polite" className="field__error">
          {toCommandError(retry.error).message}
        </p>
      ) : null}

      {entries.length === 0 ? (
        <EmptyState
          title="Очередь пуста"
          description="Загрузки появятся здесь, когда лаунчер начнёт устанавливать версию."
        />
      ) : (
        <div className="card table-panel">
          <table className="table">
            <thead>
              <tr>
                <th>Задача</th>
                <th>Состояние</th>
                <th>Файлов</th>
                <th>Размер</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {groups.map((group, index) => (
                <Fragment key={group.name}>
                  <tr>
                    <td>
                      <Button
                        variant="ghost"
                        aria-expanded={opened.has(group.name)}
                        onClick={() => toggle(group.name)}
                      >
                        <span aria-hidden="true">{opened.has(group.name) ? "⌄" : "›"}</span>{" "}
                        {taskName(group.name)}
                      </Button>
                    </td>
                    <td>
                      <span className={badgeOf(group.state)}>{STATE_LABELS[group.state]}</span>
                    </td>
                    <td className="mono">{group.files.length}</td>
                    <td className="mono">{formatBytes(group.bytes)}</td>
                    <td>
                      <div className="table__actions">
                        {group.state === "failed" ? (
                          <span className="badge badge--warning">
                            {group.files.filter((file) => file.state === "failed").length} неудачных
                          </span>
                        ) : null}
                      </div>
                    </td>
                  </tr>
                  {opened.has(group.name) ? (
                    <tr id={`download-task-${index}`}>
                      <td colSpan={5}>
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
                            {group.files.map((file) => (
                              <tr key={file.url}>
                                <td className="mono selectable">{nameOf(file)}</td>
                                <td>
                                  <span className={badgeOf(file.state)}>
                                    {STATE_LABELS[file.state]}
                                  </span>
                                </td>
                                <td className="mono">{formatBytes(file.bytes_done)}</td>
                                <td className="mono">{file.attempts}</td>
                                <td>
                                  <div className="table__actions">
                                    {file.state === "failed" ? (
                                      <Button
                                        variant="primary"
                                        busy={retry.isPending && retry.variables === file.url}
                                        onClick={() => retry.mutate(file.url)}
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
                      </td>
                    </tr>
                  ) : null}
                </Fragment>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <section className="card">
        <div className="card__title">Что такое очередь</div>
        <p className="field__hint">
          Каждая задача — одна установка: версия или среда Java. Под ней лежат файлы, которые
          лаунчер для неё качал. Неудачный файл можно перекачать ещё раз: файл пишется рядом с
          нужным местом и переносится на место только тогда, когда его целиком скачали и сверили с
          контрольной суммой.
        </p>
      </section>
    </div>
  );
}

/**
 * Group the entries of the queue by the task they belong to.
 *
 * The order of the tasks is the order of their first entry, so that the
 * queue keeps the order in which the work was started.
 */
function groupByTask(entries: DownloadEntry[]): TaskGroup[] {
  const groups = new Map<string, TaskGroup>();

  for (const entry of entries) {
    const name = entry.task ?? "";
    const group = groups.get(name);
    if (group) {
      group.files.push(entry);
      group.bytes += entry.bytes_done;
      if (RANK[entry.state] > RANK[group.state]) {
        group.state = entry.state;
      }
    } else {
      groups.set(name, {
        name,
        files: [entry],
        state: entry.state,
        bytes: entry.bytes_done,
      });
    }
  }

  return [...groups.values()];
}

/** The name of a task, for a player to read. */
function taskName(name: string): string {
  return name || "Без задачи";
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

/**
 * How much a state says about the task, from the least to the most.
 *
 * A task with a failed file is a failed task, even when the rest of its
 * files are done.
 */
const RANK: Record<DownloadState, number> = {
  completed: 0,
  queued: 1,
  downloading: 2,
  failed: 3,
};
