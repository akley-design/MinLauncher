import { useCallback, useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";
import { open } from "@tauri-apps/plugin-shell";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import type { AuthChangeEvent, Session } from "@supabase/supabase-js";

import { launcherAuth, parseMinLauncherUrl } from "../auth";
import { Button } from "./Button";
import { listAccounts, redeemMinecraftHandoff, upsertMinLauncherAccount } from "../ipc/client";
import { queryKeys } from "../ipc/queries";
import { syncGameAccounts } from "../lib/account-sync";

type Provider = "google" | "discord";

export function MinLauncherAccount() {
  const [session, setSession] = useState<Session | null>(null);
  const [nickname, setNickname] = useState("");
  const [busy, setBusy] = useState(false);
  const [waitingForBrowser, setWaitingForBrowser] = useState(false);
  const [error, setError] = useState("");
  const [syncPassword, setSyncPassword] = useState("");
  const [syncing, setSyncing] = useState(false);
  const [syncMessage, setSyncMessage] = useState("");
  const knownAccountIds = useRef<string[] | null>(null);
  const accountSignature = useRef<string | null>(null);
  const activeUserId = useRef<string | null>(null);
  const syncLock = useRef(false);
  const queryClient = useQueryClient();
  const localAccounts = useQuery({
    queryKey: queryKeys.accounts(),
    queryFn: listAccounts,
    enabled: Boolean(session && syncPassword.length >= 12 && isTauri()),
  });
  const processedCodes = useRef(new Set<string>());

  useEffect(() => {
    const configuredAuth = launcherAuth;
    if (!configuredAuth || !isTauri()) return;
    const auth = configuredAuth as NonNullable<typeof launcherAuth>;
    let active = true;
    const {
      data: { subscription },
    } = auth.auth.onAuthStateChange((_event: AuthChangeEvent, nextSession) => {
      const nextUserId = nextSession?.user.id ?? null;
      if (activeUserId.current !== nextUserId) {
        setSyncPassword("");
        setSyncMessage("");
        knownAccountIds.current = null;
        accountSignature.current = null;
        activeUserId.current = nextUserId;
      }
      if (active) setSession(nextSession);
    });

    async function handleUrl(raw: string) {
      const action = parseMinLauncherUrl(raw);
      if (!action) return;
      if (action.kind === "error") {
        setError(action.message);
        setBusy(false);
        setWaitingForBrowser(false);
        return;
      }
      if (processedCodes.current.has(action.code)) return;
      processedCodes.current.add(action.code);
      setBusy(true);
      setWaitingForBrowser(false);
      setError("");
      try {
        if (action.kind === "oauth") {
          const { error: exchangeError } = await auth.auth.exchangeCodeForSession(action.code);
          if (exchangeError) throw exchangeError;
        } else if (action.kind === "minecraft") {
          const accounts = await redeemMinecraftHandoff(action.code);
          queryClient.setQueryData(queryKeys.accounts(), accounts);
          setNickname(
            accounts.accounts.find((account) => account.kind === "microsoft")?.username ?? "",
          );
        } else {
          const { data, error: handoffError } = await auth.functions.invoke("redeem-handoff", {
            body: { code: action.code },
          });
          if (handoffError) throw handoffError;
          if (typeof data?.token_hash !== "string")
            throw new Error("Не удалось подтвердить вход с сайта.");
          const { error: verifyError } = await auth.auth.verifyOtp({
            type: "magiclink",
            token_hash: data.token_hash,
          });
          if (verifyError) throw verifyError;
          const { data: profileData, error: profileError } = await auth.functions.invoke(
            "launcher-nickname",
            {
              body: { operation: "get_profile" },
            },
          );
          if (profileError) throw profileError;
          const profile = profileData?.profile;
          if (!profile?.nickname || !profile?.player_uuid)
            throw new Error("В аккаунте сайта ещё не выбран ник MinLauncher.");
          const accounts = await upsertMinLauncherAccount(profile.nickname, profile.player_uuid);
          queryClient.setQueryData(queryKeys.accounts(), accounts);
          if (active) setNickname(profile.nickname);
        }
      } catch (cause) {
        processedCodes.current.delete(action.code);
        setError(
          cause instanceof Error ? cause.message : "Не удалось войти в аккаунт MinLauncher.",
        );
      } finally {
        setBusy(false);
      }
    }

    let stopListening: (() => void) | undefined;
    void (async () => {
      try {
        const stop = await onOpenUrl((urls) => urls.forEach((url) => void handleUrl(url)));
        if (!active) {
          stop();
          return;
        }
        stopListening = stop;

        // The callback can arrive while the event listener is being
        // registered. Read the plugin's current URL only after subscribing
        // so a cold-start or early second-instance event cannot be lost.
        const urls = await getCurrent();
        urls?.forEach((url) => void handleUrl(url));
      } catch (cause) {
        if (active) {
          setError(
            cause instanceof Error ? cause.message : "Не удалось получить возврат из браузера.",
          );
          setWaitingForBrowser(false);
        }
      }
    })();

    void auth.auth
      .getSession()
      .then(({ data, error: sessionError }) => {
        if (active) {
          activeUserId.current = data.session?.user.id ?? null;
          setSession(data.session);
          if (sessionError) setError(sessionError.message);
        }
      })
      .catch((cause: unknown) => {
        if (active)
          setError(cause instanceof Error ? cause.message : "Не удалось прочитать сессию.");
      });

    return () => {
      active = false;
      stopListening?.();
      subscription.unsubscribe();
    };
  }, [queryClient]);

  useEffect(() => {
    const userId = session?.user.id;
    if (!userId || !launcherAuth || !isTauri()) return;
    let active = true;
    void launcherAuth.functions
      .invoke("launcher-nickname", { body: { operation: "get_profile" } })
      .then(async ({ data, error: profileError }) => {
        if (profileError) throw profileError;
        const profile = data?.profile;
        if (!profile?.nickname || !profile?.player_uuid)
          throw new Error("Сначала выберите ник MinLauncher на сайте.");
        if (active) {
          setNickname(profile.nickname);
          const accounts = await upsertMinLauncherAccount(profile.nickname, profile.player_uuid);
          queryClient.setQueryData(queryKeys.accounts(), accounts);
        }
      })
      .catch((cause: unknown) => {
        if (active)
          setError(
            cause instanceof Error
              ? cause.message
              : "Не удалось синхронизировать профиль MinLauncher.",
          );
      });
    return () => {
      active = false;
    };
  }, [queryClient, session?.user.id]);

  const runAccountSync = useCallback(
    async (deletedAccountIds: string[] = []) => {
      if (!session || !launcherAuth || syncPassword.length < 12 || syncLock.current) return;
      syncLock.current = true;
      setSyncing(true);
      setSyncMessage("");
      try {
        const ids = await syncGameAccounts(
          launcherAuth,
          session.user.id,
          syncPassword,
          deletedAccountIds,
        );
        const latest = await listAccounts();
        knownAccountIds.current = ids;
        accountSignature.current = JSON.stringify(latest.accounts);
        queryClient.setQueryData(queryKeys.accounts(), latest);
        setSyncMessage(`Синхронизировано аккаунтов: ${ids.length}.`);
      } catch (cause) {
        setSyncMessage(
          cause instanceof Error ? cause.message : "Не удалось синхронизировать аккаунты.",
        );
      } finally {
        syncLock.current = false;
        setSyncing(false);
      }
    },
    [session, syncPassword, queryClient],
  );

  useEffect(() => {
    const userId = session?.user.id;
    if (!userId || !localAccounts.data || knownAccountIds.current === null) return;
    const signature = JSON.stringify(localAccounts.data.accounts);
    if (accountSignature.current === signature) return;
    const currentIds = localAccounts.data.accounts.map((account) => account.id);
    const removedIds = knownAccountIds.current.filter((id) => !currentIds.includes(id));
    accountSignature.current = signature;
    void runAccountSync(removedIds);
  }, [localAccounts.data, session?.user.id, runAccountSync]);

  async function signIn(provider: Provider) {
    if (!launcherAuth) {
      setError("Вход пока недоступен в этой сборке. Попробуйте после обновления лаунчера.");
      return;
    }
    setBusy(true);
    setError("");
    try {
      const { data, error: signInError } = await launcherAuth.auth.signInWithOAuth({
        provider,
        options: { redirectTo: "minlauncher://auth/callback", skipBrowserRedirect: true },
      });
      if (signInError) throw signInError;
      if (!data.url) throw new Error("Провайдер не вернул адрес входа.");
      setWaitingForBrowser(true);
      await open(data.url);
    } catch (cause) {
      setWaitingForBrowser(false);
      setError(cause instanceof Error ? cause.message : "Не удалось открыть страницу входа.");
    } finally {
      setBusy(false);
    }
  }

  async function signOut() {
    if (!launcherAuth) return;
    setBusy(true);
    setWaitingForBrowser(false);
    setError("");
    try {
      const { error: signOutError } = await launcherAuth.auth.signOut({ scope: "local" });
      if (signOutError) setError(signOutError.message);
      else {
        setSyncPassword("");
        setSyncMessage("");
        knownAccountIds.current = null;
        accountSignature.current = null;
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Не удалось выйти из аккаунта.");
    } finally {
      setBusy(false);
    }
  }

  const displayName = String(
    nickname ||
      (session?.user.user_metadata.display_name ??
        session?.user.user_metadata.full_name ??
        session?.user.email ??
        "Игрок MinLauncher"),
  );

  return (
    <section className="card minlauncher-account" aria-labelledby="minlauncher-account-title">
      <div className="card__title" id="minlauncher-account-title">
        Аккаунт MinLauncher
      </div>
      {!isTauri() ? (
        <p className="field__hint">Вход доступен в установленном приложении MinLauncher.</p>
      ) : session ? (
        <div className="minlauncher-account__profile">
          <span className="accounts__avatar" aria-hidden="true">
            {displayName.slice(0, 1).toUpperCase()}
          </span>
          <span>
            <strong>{displayName}</strong>
            <small>{session.user.email ?? "Почта не предоставлена"}</small>
          </span>
          <Button variant="ghost" disabled={busy} onClick={() => void signOut()}>
            {busy ? "Выходим…" : "Выйти"}
          </Button>
          <form
            className="minlauncher-account__sync"
            onSubmit={(event) => {
              event.preventDefault();
              void runAccountSync();
            }}
          >
            <label className="field__label" htmlFor="account-sync-password">
              Пароль синхронизации
            </label>
            <input
              id="account-sync-password"
              className="input"
              type="password"
              autoComplete="new-password"
              minLength={12}
              value={syncPassword}
              onChange={(event) => setSyncPassword(event.target.value)}
              placeholder="Минимум 12 символов"
            />
            <p className="field__hint">
              Пароль нужен только для шифрования и расшифровки облачной копии. Если забыть его,
              восстановить токены будет нельзя.
            </p>
            <Button type="submit" busy={syncing} disabled={syncPassword.length < 12 || syncing}>
              {syncing ? "Синхронизируем…" : "Синхронизировать игровые аккаунты"}
            </Button>
            {syncMessage ? (
              <p className="field__hint" role="status">
                {syncMessage}
              </p>
            ) : null}
          </form>
        </div>
      ) : (
        <>
          <p className="field__hint">
            После выбора ника на сайте профиль появится в списке игровых аккаунтов. Он запускает
            оффлайн-профиль, для которого не нужна купленная Java. Он подойдёт для серверов,
            разрешающих вход без проверки лицензии; серверам с проверкой Microsoft нужен
            лицензированный аккаунт Minecraft Java.
          </p>
          <div className="minlauncher-account__providers">
            <Button busy={busy} disabled={waitingForBrowser} onClick={() => void signIn("google")}>
              Войти через Google
            </Button>
            <Button
              variant="ghost"
              busy={busy}
              disabled={waitingForBrowser}
              onClick={() => void signIn("discord")}
            >
              Войти через Discord
            </Button>
          </div>
        </>
      )}
      {waitingForBrowser ? (
        <p className="field__hint" role="status">
          Ожидаем подтверждение в браузере…{" "}
          <button
            className="minlauncher-account__cancel"
            type="button"
            onClick={() => setWaitingForBrowser(false)}
          >
            Отменить
          </button>
        </p>
      ) : null}
      {busy && !session ? (
        <p className="field__hint" role="status">
          Подготавливаем защищённый вход…
        </p>
      ) : null}
      {error ? (
        <p className="field__error" role="alert">
          {error}
        </p>
      ) : null}
    </section>
  );
}
