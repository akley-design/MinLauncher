/**
 * The screen of the versions.
 *
 * The list comes from the database, not from the network: a launcher
 * that has to reach the internet before it can show anything is a
 * launcher that is slow every single time it is opened. The button
 * "Обновить" asks the backend to fetch the manifest, and the list
 * changes under the player's fingers while the network answers.
 *
 * The button "Установить" starts an installation. The installation
 * takes minutes, so its progress is shown in the frame of the launcher
 * and not only here — the player can go to another screen while the
 * files come in.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo, useState } from "react";

import { Button } from "../components/Button";
import { EmptyState, ErrorState, LoadingState } from "../components/States";
import { installVersion, listVersions, refreshManifest, toCommandError } from "../ipc/client";
import { queryKeys, useLaunchGame } from "../ipc/queries";
import type { VersionKind } from "../ipc/types";
import { formatBytes, formatTime } from "../lib/format";
import { useGameStore } from "../stores/game";
import { progressOf, useInstallStore } from "../stores/install";

/** The screen of the versions. */
export function VersionsRoute() {
  const queryClient = useQueryClient();
  const [filter, setFilter] = useState<VersionKind | "all">("all");
  const [search, setSearch] = useState("");

  const versions = useQuery({
    queryKey: queryKeys.versions(),
    queryFn: listVersions,
  });

  const refresh = useMutation({
    mutationFn: refreshManifest,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.versions() });
      void queryClient.invalidateQueries({ queryKey: queryKeys.manifest() });
    },
  });

  const install = useMutation({
    mutationFn: installVersion,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.versions() });
    },
  });

  // The game is started from here as well: the version the player wants
  // to play is often the one they are looking at.
  const launch = useLaunchGame();
  const running = useGameStore((state) => state.running);
  const starting = useGameStore((state) => state.starting);

  // The state of the installation is in a store of its own, because the
  // frame of the launcher shows it too.
  const installing = useInstallStore((state) => state.versionId);
  const installState = useInstallStore((state) => state);
  const progress = Math.round(progressOf(installState) * 100);

  const shown = useMemo(() => {
    const all = versions.data?.versions ?? [];
    const needle = search.trim().toLowerCase();

    return all.filter((version) => {
      if (filter !== "all" && version.kind !== filter) {
        return false;
      }
      if (needle && !version.id.toLowerCase().includes(needle)) {
        return false;
      }
      return true;
    });
  }, [versions.data, filter, search]);

  if (versions.isPending) {
    return <LoadingState label="Читаем список версий…" />;
  }

  if (versions.isError) {
    return <ErrorState error={versions.error} />;
  }

  return (
    <div className="screen">
      <div className="screen__header">
        <div>
          <h1 className="screen__title">Версии</h1>
          <p className="screen__subtitle">
            {versions.data?.installed_count ?? 0} установлено, {versions.data?.versions.length ?? 0}{" "}
            известно
          </p>
        </div>
        <div className="screen__actions">
          <Button busy={refresh.isPending} onClick={() => refresh.mutate()}>
            Обновить
          </Button>
        </div>
      </div>

      {refresh.isError ? (
        <p className="field__error">{toCommandError(refresh.error).message}</p>
      ) : null}

      {install.isError ? (
        <p className="field__error">{toCommandError(install.error).message}</p>
      ) : null}

      {launch.isError ? (
        <p className="field__error">{toCommandError(launch.error).message}</p>
      ) : null}

      {installing ? (
        <div className="progress" role="status">
          <div className="progress__head">
            <span className="progress__title">Установка {installing}</span>
            <span className="progress__value">
              {installState.completed} из {installState.total} · {formatBytes(installState.bytes)} ·{" "}
              {progress}%
            </span>
          </div>
          <div className="progress__track">
            <div className="progress__fill" style={{ width: `${progress}%` }} />
          </div>
        </div>
      ) : null}

      <div className="toolbar">
        <input
          className="input"
          style={{ maxWidth: 280 }}
          placeholder="Поиск по идентификатору"
          value={search}
          onChange={(event) => setSearch(event.target.value)}
        />
        <div className="filters">
          {(["all", "release", "snapshot", "old_beta", "old_alpha"] as const).map((kind) => (
            <button
              key={kind}
              type="button"
              className={filter === kind ? "filters__chip filters__chip--active" : "filters__chip"}
              onClick={() => setFilter(kind)}
            >
              {KIND_LABELS[kind]}
            </button>
          ))}
        </div>
      </div>

      {shown.length === 0 ? (
        <EmptyState
          title="Ничего не найдено"
          description="Измените фильтр или обновите список версий."
        />
      ) : (
        <div className="card" style={{ padding: 0 }}>
          <table className="table">
            <thead>
              <tr>
                <th>Версия</th>
                <th>Тип</th>
                <th>Java</th>
                <th>Опубликована</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {shown.map((version) => (
                <tr key={version.id}>
                  <td className="mono">{version.id}</td>
                  <td>
                    <span className="badge">{KIND_LABELS[version.kind]}</span>
                  </td>
                  <td>{version.java_major ?? "—"}</td>
                  <td className="selectable">{formatTime(version.release_time)}</td>
                  <td>
                    <div className="table__actions">
                      {version.installed ? (
                        <>
                          <span className="badge badge--success">Установлена</span>
                          <Button
                            busy={launch.isPending && launch.variables === version.id}
                            disabled={Boolean(running) || starting}
                            onClick={() => launch.mutate(version.id)}
                          >
                            Играть
                          </Button>
                        </>
                      ) : (
                        <Button
                          variant="primary"
                          busy={install.isPending && installing === version.id}
                          disabled={installing !== null}
                          onClick={() => install.mutate({ version_id: version.id })}
                        >
                          Установить
                        </Button>
                      )}
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

/** The names of the kinds, in Russian. */
const KIND_LABELS: Record<VersionKind | "all", string> = {
  all: "Все",
  release: "Релиз",
  snapshot: "Снапшот",
  old_beta: "Старые beta",
  old_alpha: "Старые alpha",
};
