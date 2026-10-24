/** Checks for a launcher update at startup and offers a signed install. */

import { useMutation, useQuery } from "@tanstack/react-query";

import { Button } from "../components/Button";
import { checkForUpdate, installUpdate, toCommandError } from "../ipc/client";

const UPDATE_QUERY_KEY = ["launcher", "update"] as const;

export function LauncherUpdateNotice() {
  const update = useQuery({
    queryKey: UPDATE_QUERY_KEY,
    queryFn: checkForUpdate,
    staleTime: 60_000,
  });
  const install = useMutation({
    mutationFn: () => installUpdate(update.data?.raw_json),
  });

  if (update.data?.available !== true || update.data.raw_json === null) return null;

  return (
    <section className="update-notice" role="alert" aria-live="polite">
      <div className="update-notice__copy">
        <strong>Доступно обновление MinLauncher {update.data.version}</strong>
        {update.data.notes ? <span>{update.data.notes}</span> : null}
        {install.isError ? (
          <span className="field__error">{toCommandError(install.error).message}</span>
        ) : null}
      </div>
      <Button variant="primary" busy={install.isPending} onClick={() => install.mutate()}>
        Обновить сейчас
      </Button>
    </section>
  );
}
