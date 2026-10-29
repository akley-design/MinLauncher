/** Checks for a launcher update at startup and offers a signed install. */

import { useMutation, useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";

import { Button } from "../components/Button";
import { checkForUpdate, installUpdate, toCommandError } from "../ipc/client";
import { queryKeys } from "../ipc/queries";

export function LauncherUpdateNotice() {
  const dialog = useRef<HTMLDialogElement>(null);
  const [dismissed, setDismissed] = useState(false);
  const update = useQuery({
    queryKey: queryKeys.launcherUpdate(),
    queryFn: checkForUpdate,
    staleTime: 60_000,
  });
  const install = useMutation({
    mutationFn: () => installUpdate(update.data?.raw_json),
  });

  const available = update.data?.available === true && update.data.raw_json !== null;

  useEffect(() => {
    if (available && !dismissed && !dialog.current?.open) dialog.current?.showModal();
    if ((!available || dismissed) && dialog.current?.open) dialog.current.close();
  }, [available, dismissed]);

  return (
    <dialog
      ref={dialog}
      className="confirm-dialog"
      aria-labelledby="launcher-update-title"
      onCancel={() => setDismissed(true)}
      onClose={() => setDismissed(true)}
    >
      <div className="confirm-dialog__content">
        <h2 className="confirm-dialog__title" id="launcher-update-title">
          Обновление MinLauncher {update.data?.version}
        </h2>
        {update.data?.notes ? (
          <p className="confirm-dialog__description" style={{ whiteSpace: "pre-wrap" }}>
            {update.data.notes}
          </p>
        ) : (
          <p className="confirm-dialog__description">Описание обновления не указано.</p>
        )}
        {install.isError ? (
          <p role="alert" className="field__error">
            {toCommandError(install.error).message}
          </p>
        ) : null}
        <div className="confirm-dialog__actions">
          <Button variant="ghost" onClick={() => setDismissed(true)}>
            Позже
          </Button>
          <Button variant="primary" busy={install.isPending} onClick={() => install.mutate()}>
            Обновить
          </Button>
        </div>
      </div>
    </dialog>
  );
}
