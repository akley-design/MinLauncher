/**
 * The screen of the instances.
 *
 * An instance is a world with its own directory: its saves, its mods
 * and its resource packs. Removing an instance from the launcher does
 * **not** remove the directory — the world of a player is worth more
 * than the tidiness of a launcher.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";

import { Button } from "../components/Button";
import { EmptyState, ErrorState, LoadingState } from "../components/States";
import {
  createInstance,
  listInstances,
  listVersions,
  removeInstance,
  toCommandError,
} from "../ipc/client";
import { queryKeys } from "../ipc/queries";

/** The screen of the instances. */
export function InstancesRoute() {
  const queryClient = useQueryClient();
  const [name, setName] = useState("");
  const [versionId, setVersionId] = useState("");

  const instances = useQuery({
    queryKey: queryKeys.instances(),
    queryFn: listInstances,
  });

  const versions = useQuery({
    queryKey: queryKeys.versions(),
    queryFn: listVersions,
  });

  const create = useMutation({
    mutationFn: () => createInstance(name.trim(), versionId),
    onSuccess: () => {
      setName("");
      void queryClient.invalidateQueries({ queryKey: queryKeys.instances() });
    },
  });

  const remove = useMutation({
    mutationFn: removeInstance,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.instances() });
    },
  });

  const installed = (versions.data?.versions ?? []).filter((version) => version.installed);

  if (instances.isPending) {
    return <LoadingState label="Читаем инстансы…" />;
  }

  if (instances.isError) {
    return <ErrorState error={instances.error} />;
  }

  return (
    <div className="screen">
      <div className="screen__header">
        <div>
          <h1 className="screen__title">Инстансы</h1>
          <p className="screen__subtitle">
            У каждого инстанса своя папка с мирами, модами и ресурспаками
          </p>
        </div>
      </div>

      <section className="card">
        <div className="card__title">Новый инстанс</div>

        <form
          className="form-row"
          onSubmit={(event) => {
            event.preventDefault();
            if (name.trim() && versionId) {
              create.mutate();
            }
          }}
        >
          <div className="field" style={{ flex: 1 }}>
            <label className="field__label" htmlFor="instance-name">
              Название
            </label>
            <input
              id="instance-name"
              className="input"
              placeholder="Мой мир"
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          </div>

          <div className="field" style={{ flex: 1 }}>
            <label className="field__label" htmlFor="instance-version">
              Версия
            </label>
            <select
              id="instance-version"
              className="input"
              value={versionId}
              onChange={(event) => setVersionId(event.target.value)}
            >
              <option value="">—</option>
              {installed.map((version) => (
                <option key={version.id} value={version.id}>
                  {version.id}
                </option>
              ))}
            </select>
          </div>

          <Button type="submit" busy={create.isPending} disabled={!name.trim() || !versionId}>
            Создать
          </Button>
        </form>

        {create.isError ? (
          <p className="field__error" style={{ marginTop: "var(--space-3)" }}>
            {toCommandError(create.error).message}
          </p>
        ) : null}
      </section>

      {instances.data.instances.length === 0 ? (
        <EmptyState
          title="Инстансов нет"
          description="Создайте инстанс, чтобы у каждой версии была своя папка."
        />
      ) : (
        <div className="card" style={{ padding: 0 }}>
          <table className="table">
            <thead>
              <tr>
                <th>Название</th>
                <th>Версия</th>
                <th>Папка</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {instances.data.instances.map((instance) => (
                <tr key={instance.id}>
                  <td>{instance.name}</td>
                  <td className="mono">{instance.version_id}</td>
                  <td className="mono selectable">{instance.game_dir}</td>
                  <td>
                    <div className="table__actions">
                      <Button
                        variant="danger"
                        onClick={() => remove.mutate(instance.id)}
                        title="Убрать из лаунчера, папка останется"
                      >
                        Убрать
                      </Button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {remove.isError ? (
        <p className="field__error">{toCommandError(remove.error).message}</p>
      ) : null}
    </div>
  );
}
