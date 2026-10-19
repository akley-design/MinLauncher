/**
 * The screen the launcher opens on.
 *
 * It answers one question — "what do I play, and how do I start it" —
 * and it answers it without scrolling. The version that will be played
 * is chosen here, the account is chosen here, and the play button is
 * the only large thing on the screen.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";

import { Button } from "../components/Button";
import { EmptyState, ErrorState, LoadingState } from "../components/States";
import {
  addOfflineAccount,
  launchGame,
  listAccounts,
  listInstances,
  listVersions,
  selectAccount,
  stopGame,
  toCommandError,
} from "../ipc/client";
import { queryKeys } from "../ipc/queries";
import { plural } from "../lib/plural";
import { useGameStore } from "../stores/game";

/** The screen the launcher opens on. */
export function HomeRoute() {
  const queryClient = useQueryClient();
  const starting = useGameStore((state) => state.starting);
  const running = useGameStore((state) => state.running);
  const beginStart = useGameStore((state) => state.beginStart);
  const endStart = useGameStore((state) => state.endStart);
  const failed = useGameStore((state) => state.failed);

  const [newAccount, setNewAccount] = useState("");

  const versions = useQuery({
    queryKey: queryKeys.versions(),
    queryFn: listVersions,
  });

  const accounts = useQuery({
    queryKey: queryKeys.accounts(),
    queryFn: listAccounts,
  });

  const instances = useQuery({
    queryKey: queryKeys.instances(),
    queryFn: listInstances,
  });

  // The version that will be played. It is the one the settings name,
  // or the latest release, or the newest installed one.
  const chosenVersion =
    versions.data?.versions.find((version) => version.installed && version.kind === "release")
      ?.id ??
    versions.data?.versions.find((version) => version.installed)?.id ??
    versions.data?.latest_release ??
    "";

  const installed = versions.data?.versions.filter((version) => version.installed) ?? [];
  const selected = accounts.data?.accounts.find(
    (account) => account.id === accounts.data?.selected_id,
  );

  const launch = useMutation({
    mutationFn: async () => {
      if (!chosenVersion) {
        throw new Error("не выбрана версия");
      }
      return launchGame({ version_id: chosenVersion });
    },
    onMutate: () => beginStart(),
    onSuccess: (result) => {
      endStart();
      // The result carries the time the launcher spent preparing the
      // launch. It is the number M3 is built on, so it is worth
      // showing rather than throwing away.
      void queryClient.invalidateQueries({ queryKey: queryKeys.benchmarks() });
      void result;
    },
    onError: (error) => failed(toCommandError(error).message),
  });

  const stop = useMutation({
    mutationFn: stopGame,
    onSuccess: () => useGameStore.setState({ running: null }),
  });

  const chooseAccount = useMutation({
    mutationFn: selectAccount,
    onSuccess: (list) => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.accounts() });
      void list;
    },
  });

  const addAccount = useMutation({
    mutationFn: addOfflineAccount,
    onSuccess: () => {
      setNewAccount("");
      void queryClient.invalidateQueries({ queryKey: queryKeys.accounts() });
    },
  });

  if (versions.isPending) {
    return <LoadingState label="Читаем список версий…" />;
  }

  if (versions.isError) {
    return <ErrorState error={versions.error} />;
  }

  const canPlay = Boolean(chosenVersion) && Boolean(selected) && !running && !starting;

  return (
    <div className="screen">
      <div className="screen__header">
        <div>
          <h1 className="screen__title">Играть</h1>
          <p className="screen__subtitle">
            {installed.length > 0
              ? `${installed.length} ${plural(installed.length, "версия", "версии", "версий")} установлено`
              : "Ни одна версия не установлена"}
          </p>
        </div>
      </div>

      <section className="card">
        <div className="card__title">Запуск</div>

        <div className="play">
          <div className="play__info">
            <div className="play__label">Версия</div>
            <div className="play__value">{chosenVersion || "—"}</div>
            {chosenVersion ? (
              <div className="play__hint">
                {versions.data?.versions.find((version) => version.id === chosenVersion)?.java_major
                  ? `Java ${versions.data.versions.find((version) => version.id === chosenVersion)?.java_major}`
                  : "Java определяется автоматически"}
              </div>
            ) : null}
          </div>

          {running ? (
            <Button variant="danger" className="button--large" onClick={() => stop.mutate()}>
              Остановить
            </Button>
          ) : (
            <Button
              variant="primary"
              className="button--large"
              disabled={!canPlay}
              busy={starting}
              onClick={() => launch.mutate()}
            >
              {starting ? "Запуск…" : "Играть"}
            </Button>
          )}
        </div>

        {launch.isError ? (
          <p className="field__error" style={{ marginTop: "var(--space-3)" }}>
            {toCommandError(launch.error).message}
          </p>
        ) : null}

        {launch.isSuccess ? (
          <p className="play__hint" style={{ marginTop: "var(--space-3)" }}>
            Подготовка заняла {launch.data.prepare_ms} мс
          </p>
        ) : null}
      </section>

      <section className="card">
        <div className="card__title">Аккаунт</div>

        {accounts.isPending ? (
          <LoadingState label="Читаем аккаунты…" />
        ) : (accounts.data?.accounts.length ?? 0) === 0 ? (
          <EmptyState
            title="Аккаунта нет"
            description="Добавьте оффлайн-аккаунт, чтобы начать игру без входа в Microsoft."
          />
        ) : (
          <ul className="accounts">
            {accounts.data?.accounts.map((account) => (
              <li key={account.id} className="accounts__item">
                <button
                  type="button"
                  className={
                    account.id === accounts.data?.selected_id
                      ? "accounts__button accounts__button--active"
                      : "accounts__button"
                  }
                  onClick={() => chooseAccount.mutate(account.id)}
                >
                  <span className="accounts__avatar" aria-hidden="true">
                    {account.username.slice(0, 1).toUpperCase()}
                  </span>
                  <span className="accounts__name">{account.username}</span>
                  <span className="badge">{account.kind}</span>
                </button>
              </li>
            ))}
          </ul>
        )}

        <form
          className="accounts__add"
          onSubmit={(event) => {
            event.preventDefault();
            if (newAccount.trim()) {
              addAccount.mutate(newAccount.trim());
            }
          }}
        >
          <input
            className="input"
            placeholder="Имя игрока"
            value={newAccount}
            onChange={(event) => setNewAccount(event.target.value)}
            maxLength={16}
          />
          <Button type="submit" busy={addAccount.isPending} disabled={!newAccount.trim()}>
            Добавить
          </Button>
        </form>

        {addAccount.isError ? (
          <p className="field__error" style={{ marginTop: "var(--space-2)" }}>
            {toCommandError(addAccount.error).message}
          </p>
        ) : null}
      </section>

      {instances.data && instances.data.instances.length > 0 ? (
        <section className="card">
          <div className="card__title">Инстансы</div>
          <ul className="accounts">
            {instances.data.instances.map((instance) => (
              <li key={instance.id} className="accounts__item">
                <div className="accounts__button accounts__button--static">
                  <span className="accounts__name">{instance.name}</span>
                  <span className="badge">{instance.version_id}</span>
                </div>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
    </div>
  );
}
