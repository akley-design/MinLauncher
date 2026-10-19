/**
 * The Java runtimes the launcher can download.
 *
 * A version of the game asks for a Java of a certain feature release.
 * The launcher can fetch the runtime of Mojang itself, so that a player
 * without a Java installation can still play.
 *
 * The list comes from the manifest of Mojang and only holds the
 * runtimes that fit the machine. A runtime that is already on the disk
 * says so, and the button of the others starts the download.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { Button } from "../components/Button";
import { ErrorState, LoadingState } from "../components/States";
import { installRuntime, listRuntimes, toCommandError } from "../ipc/client";
import { queryKeys } from "../ipc/queries";
import { formatBytes } from "../lib/format";
import { progressOf, useRuntimeStore } from "../stores/runtime";

/** The section of the settings that lists the Java runtimes. */
export function JavaRuntimes() {
  const queryClient = useQueryClient();

  const runtimes = useQuery({
    queryKey: queryKeys.runtimes(),
    queryFn: listRuntimes,
  });

  const download = useMutation({
    mutationFn: installRuntime,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.runtimes() });
    },
  });

  const component = useRuntimeStore((state) => state.component);
  const state = useRuntimeStore((state) => state);
  const progress = Math.round(progressOf(state) * 100);

  if (runtimes.isPending) {
    return <LoadingState label="Читаем список Java…" />;
  }

  if (runtimes.isError) {
    return <ErrorState error={runtimes.error} />;
  }

  return (
    <section className="card">
      <div className="card__title">Java</div>

      {download.isError ? (
        <p className="field__error">{toCommandError(download.error).message}</p>
      ) : null}
      {state.error ? <p className="field__error">{state.error}</p> : null}

      {component ? (
        <div className="progress" role="status">
          <div className="progress__head">
            <span className="progress__title">Загрузка Java {component}</span>
            <span className="progress__value">
              {state.completed} из {state.total} · {formatBytes(state.bytes)} · {progress}%
            </span>
          </div>
          <div className="progress__track">
            <div className="progress__fill" style={{ width: `${progress}%` }} />
          </div>
        </div>
      ) : null}

      {runtimes.data.length === 0 ? (
        <p className="field__hint">Mojang не публикует Java для этой системы.</p>
      ) : (
        <div className="card" style={{ padding: 0 }}>
          <table className="table">
            <thead>
              <tr>
                <th>Сборка</th>
                <th>Версия</th>
                <th>Состояние</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {runtimes.data.map((runtime) => (
                <tr key={runtime.component}>
                  <td className="mono">{runtime.component}</td>
                  <td>{runtime.version}</td>
                  <td>
                    {runtime.installed ? (
                      <span className="badge">установлена</span>
                    ) : (
                      <span className="field__hint">не загружена</span>
                    )}
                  </td>
                  <td>
                    <Button
                      variant={runtime.installed ? "ghost" : "default"}
                      busy={download.isPending && component === runtime.component}
                      disabled={download.isPending}
                      onClick={() => download.mutate(runtime.component)}
                    >
                      {runtime.installed ? "Переустановить" : "Загрузить"}
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <span className="field__hint">
        Игра сама выбирает нужную сборку Java. Загруженная копия используется прежде системной.
      </span>
    </section>
  );
}
