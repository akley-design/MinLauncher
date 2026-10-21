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
  listAccounts,
  listInstances,
  listVersions,
  selectAccount,
  stopGame,
  toCommandError,
} from "../ipc/client";
import { queryKeys, useLaunchGame } from "../ipc/queries";
import { plural } from "../lib/plural";
import { useGameStore } from "../stores/game";

/** The screen the launcher opens on. */
export function HomeRoute() {
  const queryClient = useQueryClient();
  const starting = useGameStore((state) => state.starting);
  const running = useGameStore((state) => state.running);

  const [newAccount, setNewAccount] = useState("");
  // The version the player picked on this screen. It is empty until the
  // player picks one, and then the choice is theirs — the version the
  // launcher would have picked on its own is only the default.
  const [picked, setPicked] = useState<string | null>(null);

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

  // The version that will be played. It is the one the player picked,
  // or the newest release that is installed, or the newest version that
  // is installed, or the latest release of Mojang.
  const installed = versions.data?.versions.filter((version) => version.installed) ?? [];
  const fallback =
    installed.find((version) => version.kind === "release")?.id ??
    installed[0]?.id ??
    versions.data?.latest_release ??
    "";
  const chosenVersion =
    picked && installed.some((version) => version.id === picked) ? picked : fallback;

  const selected = accounts.data?.accounts.find(
    (account) => account.id === accounts.data?.selected_id,
  );

  const launch = useLaunchGame();

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
            {installed.length > 0 ? (
              // The list of the installed versions. The version that is
              // played is the one that is picked here, and not the one
              // the launcher would have chosen on its own.
              <select
                className="input"
                style={{ maxWidth: 280 }}
                aria-label="Версия"
                value={chosenVersion}
                onChange={(event) => setPicked(event.target.value)}
              >
                {installed.map((version) => (
                  <option key={version.id} value={version.id}>
                    {version.id}
                  </option>
                ))}
              </select>
            ) : (
              <div className="play__value">{chosenVersion || "—"}</div>
            )}
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
              onClick={() => launch.mutate(chosenVersion)}
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
