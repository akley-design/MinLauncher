/**
 * The screen of the versions.
 *
 * The list comes from the database, not from the network: a launcher
 * that has to reach the internet before it can show anything is a
 * launcher that is slow every single time it is opened. The button
 * "Обновить" asks the backend to fetch the manifest, and the list
 * changes under the player's fingers while the network answers.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo, useState } from "react";

import { Button } from "../components/Button";
import { EmptyState, ErrorState, LoadingState } from "../components/States";
import { listVersions, refreshManifest, toCommandError } from "../ipc/client";
import { queryKeys } from "../ipc/queries";
import type { VersionKind } from "../ipc/types";
import { formatTime } from "../lib/format";

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
                        <span className="badge badge--success">Установлена</span>
                      ) : (
                        <span className="badge">Не установлена</span>
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
