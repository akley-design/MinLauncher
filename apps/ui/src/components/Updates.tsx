/**
 * Launcher updates. The manifest shown is the manifest installed, and
 * an unconfigured update server is a state, not an error.
 */

import { useMutation, useQuery } from "@tanstack/react-query";

import { Button } from "../components/Button";
import { checkForUpdate, installUpdate, toCommandError } from "../ipc/client";
import { queryKeys } from "../ipc/queries";

export function Updates() {
  const check = useQuery({
    queryKey: queryKeys.launcherUpdate(),
    queryFn: checkForUpdate,
    staleTime: 60_000,
  });

  const install = useMutation({
    mutationFn: () => installUpdate(check.data?.raw_json),
  });

  const update = check.data;
  const available = update?.available === true && update.raw_json !== null;

  return (
    <section className="card">
      <div className="card__title">Обновления</div>

      {check.isError ? (
        <p role="alert" aria-live="polite" className="field__error">
          {toCommandError(check.error).message}
        </p>
      ) : null}
      {install.isError ? (
        <p role="alert" aria-live="polite" className="field__error">
          {toCommandError(install.error).message}
        </p>
      ) : null}

      {update && !update.configured ? (
        <p className="field__hint">Сервер обновлений не настроен.</p>
      ) : null}

      {update && update.configured && !update.available ? (
        <p className="field__hint">Установленная версия {update.current_version} — самая свежая.</p>
      ) : null}

      {available && update.version ? (
        <>
          <p>
            Доступна версия <strong>{update.version}</strong>.
          </p>
          {update.notes ? (
            <p className="field__hint selectable" style={{ whiteSpace: "pre-wrap" }}>
              {update.notes}
            </p>
          ) : null}
        </>
      ) : null}

      <div className="form-row" style={{ marginTop: "var(--space-3)" }}>
        <Button busy={check.isFetching} onClick={() => void check.refetch()}>
          Проверить обновления
        </Button>
        {available ? (
          <Button
            variant="primary"
            busy={install.isPending}
            disabled={check.isPending}
            onClick={() => install.mutate()}
          >
            Скачать и установить
          </Button>
        ) : null}
      </div>
    </section>
  );
}
