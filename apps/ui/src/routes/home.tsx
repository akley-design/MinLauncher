/**
 * The screen the launcher opens on.
 *
 * It answers one question — "what do I play, and how do I start it" —
 * and it answers it without scrolling. The version that will be played
 * is chosen here, the account is chosen here, and the play button is
 * the only large thing on the screen.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { Button } from "../components/Button";
import { Diorama } from "@minlauncher/visuals";
import { EmptyState, ErrorState, LoadingState } from "../components/States";
import {
  addOfflineAccount,
  clearNicknameClaim,
  offlineAccountIdentity,
  installVersion,
  listAccounts,
  listInstances,
  listVersions,
  removeAccount,
  queueNicknameRelease,
  selectAccount,
  setNicknameClaimStatus,
  stopGame,
  toCommandError,
} from "../ipc/client";
import { launcherAuth } from "../auth";
import { queryKeys, useLaunchGame } from "../ipc/queries";
import type { AccountType } from "../ipc/types";
import { LAUNCH_BUDGET_MS } from "../lib/launches";
import { plural } from "../lib/plural";
import { useGameStore } from "../stores/game";

/** The screen the launcher opens on. */
export function HomeRoute() {
  const queryClient = useQueryClient();
  const starting = useGameStore((state) => state.starting);
  const running = useGameStore((state) => state.running);
  const launches = useGameStore((state) => state.launches);

  const [newAccount, setNewAccount] = useState("");
  const [nicknameConflicts, setNicknameConflicts] = useState<Record<string, string>>({});
  const reconciledAccounts = useRef(new Set<string>());
  // The version the player picked on this screen. It is empty until the
  // player picks one, and then the choice is theirs — the version the
  // launcher would have picked on its own is only the default.
  const [picked, setPicked] = useState<string | null>(null);
  const [pickedInstance, setPickedInstance] = useState<string | null>(null);
  // The field of the name of the player. The empty state of the
  // account points at it, so that the player knows where to type.
  const nameInput = useRef<HTMLInputElement>(null);

  const versions = useQuery({
    queryKey: queryKeys.versions(),
    queryFn: listVersions,
  });

  const accounts = useQuery({
    queryKey: queryKeys.accounts(),
    queryFn: listAccounts,
  });

  useEffect(() => {
    if (!launcherAuth || !accounts.data) return;
    for (const claim of accounts.data.nickname_claims) {
      if (
        claim.status !== "release_pending" ||
        reconciledAccounts.current.has(`release:${claim.account_id}`)
      )
        continue;
      reconciledAccounts.current.add(`release:${claim.account_id}`);
      void (async () => {
        const key = `ml-nickname-${claim.account_id}`;
        try {
          const token = await invoke<string | null>("identity_storage_get", { key });
          if (!token) throw new Error("Не найден токен для освобождения ника.");
          const { error } = await launcherAuth.functions.invoke("launcher-nickname", {
            body: { operation: "release_offline", claim_token: token },
          });
          if (error) throw error;
          await invoke("identity_storage_remove", { key });
          await clearNicknameClaim(claim.account_id);
          void queryClient.invalidateQueries({ queryKey: queryKeys.accounts() });
        } catch {
          reconciledAccounts.current.delete(`release:${claim.account_id}`);
        }
      })();
    }
    for (const account of accounts.data.accounts) {
      if (account.kind !== "offline" || reconciledAccounts.current.has(account.id)) continue;
      reconciledAccounts.current.add(account.id);
      void (async () => {
        const key = `ml-nickname-${account.id}`;
        try {
          const storedToken = await invoke<string | null>("identity_storage_get", { key });
          const { data, error } = await launcherAuth.functions.invoke("launcher-nickname", {
            body: {
              operation: "claim_offline",
              nickname: account.username,
              player_uuid: account.uuid,
              ...(storedToken ? { claim_token: storedToken } : {}),
            },
          });
          if (error) {
            const status = (error as { context?: Response }).context?.status;
            if (status === 409 || status === 400) {
              setNicknameConflicts((current) => ({
                ...current,
                [account.id]: "Ник занят. Создайте аккаунт с другим ником.",
              }));
              await setNicknameClaimStatus(account.id, account.username, "conflict").catch(
                () => undefined,
              );
            }
            return;
          }
          await setNicknameClaimStatus(account.id, account.username, "reserved");
          if (!storedToken && typeof data?.claim_token === "string") {
            await invoke("identity_storage_set", { key, value: data.claim_token });
          }
          setNicknameConflicts((current) => {
            const next = { ...current };
            delete next[account.id];
            return next;
          });
        } catch {
          // Existing accounts stay playable if the network or registry is unavailable.
        }
      })();
    }
  }, [accounts.data, queryClient]);

  const instances = useQuery({
    queryKey: queryKeys.instances(),
    queryFn: listInstances,
  });

  // The version that will be played. It is the one the player picked,
  // or the newest release that is installed, or the newest version that
  // is installed, or the latest release of Mojang.
  const installed = versions.data?.versions.filter((version) => version.installed) ?? [];
  const latestRelease = versions.data?.latest_release ?? "";
  const fallback =
    installed.find((version) => version.kind === "release")?.id ??
    installed[0]?.id ??
    latestRelease;
  const chosenVersion =
    picked && installed.some((version) => version.id === picked) ? picked : fallback;
  const chosenInstance = pickedInstance
    ? instances.data?.instances.find((instance) => instance.id === pickedInstance)
    : picked
      ? undefined
      : instances.data?.instances[0];
  const playVersion = chosenInstance?.version_id ?? chosenVersion;

  // The last launch of the version that is picked. The number is read
  // from the store and not from the answer of the button, so that it is
  // still there when the player comes back to this screen.
  const lastLaunch = launches.find((record) => record.versionId === playVersion);

  const selected = accounts.data?.accounts.find(
    (account) => account.id === accounts.data?.selected_id,
  );
  const heroStatus = running
    ? "МИР ЗАПУЩЕН"
    : starting
      ? "ИГРА ЗАПУСКАЕТСЯ"
      : accounts.isPending
        ? "ПРОВЕРКА АККАУНТА"
        : accounts.isError
          ? "ОШИБКА ПРОФИЛЯ"
          : !playVersion
            ? "НУЖНА ВЕРСИЯ"
            : !selected
              ? "НУЖЕН АККАУНТ"
              : "ГОТОВ К ЗАПУСКУ";

  const launch = useLaunchGame();

  const stop = useMutation({
    mutationFn: stopGame,
    onSuccess: () => useGameStore.setState({ running: null }),
  });

  const chooseAccount = useMutation({
    mutationFn: selectAccount,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.accounts() });
    },
  });

  const deleteAccount = useMutation({
    mutationFn: removeAccount,
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: queryKeys.accounts() }),
  });

  const addAccount = useMutation({
    mutationFn: async (username: string) => {
      if (!launcherAuth) throw new Error("Не настроен сервер MinLauncher.");
      const identity = await offlineAccountIdentity(username);
      const { data, error } = await launcherAuth.functions.invoke("launcher-nickname", {
        body: { operation: "claim_offline", nickname: username, player_uuid: identity.uuid },
      });
      if (error) throw error;
      if (typeof data?.claim_token !== "string")
        throw new Error("Сервер не подтвердил резервирование ника.");
      const key = `ml-nickname-${identity.id}`;
      try {
        await invoke("identity_storage_set", { key, value: data.claim_token });
        return await addOfflineAccount(username, data.claim_token);
      } catch (cause) {
        const { error: releaseError } = await launcherAuth.functions.invoke("launcher-nickname", {
          body: { operation: "release_offline", claim_token: data.claim_token },
        });
        if (releaseError) {
          await queueNicknameRelease(identity.id, username).catch(() => undefined);
        } else {
          await invoke("identity_storage_remove", { key }).catch(() => undefined);
        }
        throw cause;
      }
    },
    onSuccess: () => {
      setNewAccount("");
      void queryClient.invalidateQueries({ queryKey: queryKeys.accounts() });
    },
  });

  // A launcher with nothing installed has nothing to play, and the
  // player must not have to hunt for the screen that installs one.
  const install = useMutation({
    mutationFn: installVersion,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.versions() });
    },
  });

  if (versions.isPending) {
    return <LoadingState label="Читаем список версий…" />;
  }

  if (versions.isError) {
    return <ErrorState error={versions.error} />;
  }

  const canPlay = Boolean(playVersion) && Boolean(selected) && !running && !starting;

  return (
    <div className="screen home-screen">
      <div className="screen__header home-screen__heading">
        <div>
          <div className="home-screen__kicker">Minecraft Java Edition</div>
          <h1 className="screen__title">Играть</h1>
          <p className="screen__subtitle">
            {installed.length > 0
              ? `${installed.length} ${plural(installed.length, "версия", "версии", "версий")} установлено`
              : "Ни одна версия не установлена"}
          </p>
        </div>
      </div>

      <section className="home-hero" aria-label="Запуск Minecraft">
        <div className="home-hero__content">
          <div className="home-hero__status">
            <span className="home-hero__status-dot" aria-hidden="true" />
            {heroStatus}
          </div>
          <h2 className="home-hero__title">{chosenInstance?.name ?? "Minecraft"}</h2>
          <p className="home-hero__description">
            {selected
              ? `Привет, ${selected.username}. Выбери версию и запускай игру.`
              : "Добавь игровое имя и выбери версию."}
          </p>

          <div className="home-hero__controls">
            <label className="home-hero__version">
              <span>АКТИВНАЯ ВЕРСИЯ</span>
              {installed.length > 0 ? (
                <select
                  className="input"
                  aria-label="Версия"
                  value={chosenInstance ? `instance:${chosenInstance.id}` : chosenVersion}
                  onChange={(event) => {
                    const value = event.target.value;
                    if (value.startsWith("instance:")) {
                      setPickedInstance(value.slice("instance:".length));
                      setPicked(null);
                    } else {
                      setPicked(value);
                      setPickedInstance(null);
                    }
                  }}
                >
                  {(instances.data?.instances ?? []).map((instance) => (
                    <option key={instance.id} value={`instance:${instance.id}`}>
                      {instance.name} · {instance.version_id}
                    </option>
                  ))}
                  {installed.map((version) => (
                    <option key={version.id} value={version.id}>
                      {version.id}
                    </option>
                  ))}
                </select>
              ) : (
                <strong>{chosenVersion || "Нет установленной версии"}</strong>
              )}
            </label>
            <section className="home-game-account">
              <div className="card__title">Игровой аккаунт</div>

              {accounts.isPending ? (
                <LoadingState label="Читаем аккаунты…" />
              ) : accounts.isError ? (
                <ErrorState error={accounts.error} />
              ) : (accounts.data?.accounts.length ?? 0) === 0 ? (
                <EmptyState
                  title="Аккаунта нет"
                  description="Добавьте оффлайн-аккаунт, чтобы играть без купленной Java на серверах, где разрешён вход без проверки лицензии."
                  action={
                    <Button onClick={() => nameInput.current?.focus()}>Добавить аккаунт</Button>
                  }
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
                        <span className={`badge badge--account badge--account-${account.kind}`}>
                          {ACCOUNT_KIND_LABELS[account.kind]}
                        </span>
                      </button>
                      <Button variant="ghost" onClick={() => deleteAccount.mutate(account.id)}>
                        Удалить
                      </Button>
                      {accounts.data?.nickname_claims.find(
                        (claim) => claim.account_id === account.id,
                      )?.status === "conflict" ? (
                        <small className="field__error">
                          Ник занят. Удалите профиль и создайте новый с другим ником.
                        </small>
                      ) : null}
                    </li>
                  ))}
                </ul>
              )}

              {Object.entries(nicknameConflicts).map(([accountId, message]) => (
                <p key={accountId} className="field__error" role="status">
                  {message}
                </p>
              ))}

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
                  ref={nameInput}
                  name="playerName"
                  aria-label="Имя игрока"
                  autoComplete="off"
                  spellCheck={false}
                  className="input"
                  placeholder="Например, Steve…"
                  value={newAccount}
                  onChange={(event) => setNewAccount(event.target.value)}
                  maxLength={16}
                />
                <Button type="submit" busy={addAccount.isPending} disabled={!newAccount.trim()}>
                  Добавить
                </Button>
              </form>
              <p className="field__hint">
                Новый ник резервируется в MinLauncher через интернет. Уже добавленные аккаунты
                запускаются без сети. Серверы с проверкой Microsoft требуют лицензию Minecraft Java.
              </p>

              {addAccount.isError ? (
                <p
                  role="alert"
                  aria-live="polite"
                  className="field__error"
                  style={{ marginTop: "var(--space-2)" }}
                >
                  {toCommandError(addAccount.error).message}
                </p>
              ) : null}
            </section>
            {installed.length === 0 ? (
              <Button
                variant="primary"
                className="button--large home-hero__play"
                busy={install.isPending}
                disabled={!latestRelease}
                onClick={() => install.mutate({ version_id: latestRelease })}
              >
                {latestRelease ? `Установить ${latestRelease}` : "Установить"}
                <span aria-hidden="true">↓</span>
              </Button>
            ) : running ? (
              <Button
                variant="danger"
                className="button--large home-hero__play"
                onClick={() => stop.mutate()}
              >
                Остановить игру <span aria-hidden="true">■</span>
              </Button>
            ) : (
              <Button
                variant="primary"
                className="button--large home-hero__play"
                disabled={!canPlay}
                busy={starting}
                onClick={() =>
                  launch.mutate(
                    chosenInstance
                      ? { version_id: chosenInstance.version_id, instance_id: chosenInstance.id }
                      : chosenVersion,
                  )
                }
              >
                {starting ? "Запуск…" : "Играть"} <span aria-hidden="true">↗</span>
              </Button>
            )}
          </div>

          {playVersion ? (
            <div className="home-hero__specs">
              <span>
                <b>MC</b> {playVersion}
              </span>
              <span>
                <b>JAVA</b>{" "}
                {versions.data?.versions.find((version) => version.id === playVersion)
                  ?.java_major ?? "AUTO"}
              </span>
              <span>
                <b>СТАТУС</b> {playVersion && selected ? "ГОТОВ" : "НЕ ГОТОВ"}
              </span>
            </div>
          ) : null}
          {launch.isError ? (
            <p role="alert" aria-live="polite" className="field__error home-hero__error">
              {toCommandError(launch.error).message}
            </p>
          ) : null}
          {lastLaunch ? (
            <p className="home-hero__last-launch">
              Подготовка {lastLaunch.versionId} заняла {lastLaunch.prepareMs} мс
              {lastLaunch.prepareMs > LAUNCH_BUDGET_MS ? " · дольше 5 с" : ""}
            </p>
          ) : null}
        </div>
        <div className="home-hero__scene">
          <div className="home-hero__scene-label">
            <span>Верхний мир</span>
          </div>
          <Diorama
            variant="island"
            mode="launcher"
            motion={running || starting ? "paused" : "active"}
          />
          <div className="home-hero__caption">
            <span>Дом у реки</span>
          </div>
        </div>
      </section>

      {installed.length === 0 ? (
        <section className="card">
          <EmptyState
            title="Установите версию"
            description="Лаунчер скачает игру, библиотеки и ассеты. После этого версию можно будет запустить."
          />
        </section>
      ) : null}

      {install.isError ? (
        <p role="alert" aria-live="polite" className="field__error">
          {toCommandError(install.error).message}
        </p>
      ) : null}

      <div
        className={
          instances.data && instances.data.instances.length > 0
            ? "home-screen__lower"
            : "home-screen__lower home-screen__lower--single"
        }
      >
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
    </div>
  );
}

const ACCOUNT_KIND_LABELS: Record<AccountType, string> = {
  microsoft: "Microsoft",
  mojang: "Mojang",
  offline: "Оффлайн",
  minlauncher: "MinLauncher",
  third_party: "Другой",
};
