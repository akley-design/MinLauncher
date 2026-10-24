/**
 * The section of the settings that updates the launcher itself.
 *
 * The launcher asks a server what the newest release is, shows what it
 * found, and puts it in place when the player says so. The manifest of
 * the release travels back to the backend with the request to install
 * it, so that the version that is put in place is the one that was
 * shown.
 *
 * A launcher that has no server of the updates to ask says so. That is
 * a state of the launcher, not a failure of it, and it is shown as
 * such: the button stays, the check can be repeated later.
 */

import { useMutation, useQuery } from "@tanstack/react-query";

import { Button } from "../components/Button";
import { checkForUpdate, installUpdate, toCommandError } from "../ipc/client";

const UPDATE_QUERY_KEY = ["launcher", "update"] as const;

/** The section of the settings that updates the launcher itself. */
export function Updates() {
  const check = useQuery({
    queryKey: UPDATE_QUERY_KEY,
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
