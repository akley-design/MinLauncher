/**
 * The screen of the instances.
 *
 * An instance is a world with its own directory: its saves, its mods
 * and its resource packs. Removing an instance from the launcher does
 * **not** remove the directory — the world of a player is worth more
 * than the tidiness of a launcher.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";

import { Button } from "../components/Button";
import { EmptyState, ErrorState, LoadingState } from "../components/States";
import {
  createInstance,
  importForgeInstance,
  installPerformanceMods,
  installVersion,
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
  const [removingId, setRemovingId] = useState<string | null>(null);
  const removalDialog = useRef<HTMLDialogElement>(null);

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

  const importForge = useMutation({
    mutationFn: async () => {
      await installVersion({ version_id: "1.20.1" });
      return importForgeInstance("Forge 1.20.1", "Forge 1.20.1 · MinLauncher");
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.instances() });
      void queryClient.invalidateQueries({ queryKey: queryKeys.versions() });
    },
  });

  const installFpsBundle = useMutation({ mutationFn: installPerformanceMods });

  const installed = (versions.data?.versions ?? []).filter((version) => version.installed);
  const removing = instances.data?.instances.find((instance) => instance.id === removingId);

  useEffect(() => {
    const dialog = removalDialog.current;
    if (removing && dialog && !dialog.open) dialog.showModal();
    if (!removing && dialog?.open) dialog.close();
  }, [removing]);

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
          <div className="screen__eyebrow">Твои миры и сборки</div>
          <h1 className="screen__title">Инстансы</h1>
          <p className="screen__subtitle">
            У каждого инстанса своя папка с мирами, модами и ресурспаками
          </p>
        </div>
      </div>

      <div className="instances-tools">
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
                name="instanceName"
                autoComplete="off"
                className="input"
                placeholder="Например, мой мир…"
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
                name="instanceVersion"
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
            <p
              role="alert"
              aria-live="polite"
              className="field__error"
              style={{ marginTop: "var(--space-3)" }}
            >
              {toCommandError(create.error).message}
            </p>
          ) : null}
        </section>

        <section className="card instances-tools__import">
          <div className="card__title">Перенести Forge 1.20.1</div>
          <p className="instances-tools__description">
            Создать отдельную копию профиля, модов, настроек и миров. Исходная папка Minecraft
            останется нетронутой. В копию можно поставить проверенный набор Forge‑оптимизаций.
          </p>
          <Button
            variant="primary"
            busy={importForge.isPending}
            onClick={() => importForge.mutate()}
          >
            {importForge.isPending ? "Подготовка Forge…" : "Импортировать профиль"}
          </Button>
          {importForge.isError ? (
            <p
              role="alert"
              aria-live="polite"
              className="field__error"
              style={{ marginTop: "var(--space-3)" }}
            >
              {toCommandError(importForge.error).message}
            </p>
          ) : null}
        </section>
      </div>

      {instances.data.instances.length === 0 ? (
        <EmptyState
          title="Инстансов нет"
          description="Создайте инстанс, чтобы у каждой версии была своя папка."
        />
      ) : (
        <div className="card table-panel">
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
                      {instance.version_id.startsWith("1.20.1-forge-") ? (
                        <Button
                          variant="ghost"
                          busy={
                            installFpsBundle.isPending && installFpsBundle.variables === instance.id
                          }
                          onClick={() => installFpsBundle.mutate(instance.id)}
                          title="Установить Embeddium и совместимые клиентские оптимизации в копию"
                        >
                          FPS‑моды
                        </Button>
                      ) : null}
                      <Button
                        variant="danger"
                        onClick={() => setRemovingId(instance.id)}
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

      {installFpsBundle.isSuccess ? (
        <p className="field__hint">
          FPS‑набор готов: {installFpsBundle.data.installed.length} установлено,{" "}
          {installFpsBundle.data.already_present.length} уже было. Он действует только в инстансе{" "}
          {installFpsBundle.data.instance_id}.
        </p>
      ) : null}
      {installFpsBundle.isError ? (
        <p role="alert" aria-live="polite" className="field__error">
          {toCommandError(installFpsBundle.error).message}
        </p>
      ) : null}

      {remove.isError ? (
        <p role="alert" aria-live="polite" className="field__error">
          {toCommandError(remove.error).message}
        </p>
      ) : null}
      <dialog
        ref={removalDialog}
        className="confirm-dialog"
        aria-labelledby="remove-instance-title"
        onCancel={() => setRemovingId(null)}
        onClose={() => setRemovingId(null)}
      >
        <div className="confirm-dialog__content">
          <span className="confirm-dialog__icon" aria-hidden="true">
            !
          </span>
          <h2 className="confirm-dialog__title" id="remove-instance-title">
            Убрать инстанс?
          </h2>
          <p className="confirm-dialog__description">
            Запись «{removing?.name}» исчезнет из лаунчера. Папка, миры и моды останутся на диске.
          </p>
          <div className="confirm-dialog__actions">
            <Button variant="ghost" onClick={() => setRemovingId(null)}>
              Оставить
            </Button>
            <Button
              variant="danger"
              onClick={() => {
                if (removingId) remove.mutate(removingId);
                setRemovingId(null);
              }}
            >
              Убрать запись
            </Button>
          </div>
        </div>
      </dialog>
    </div>
  );
}
