import { useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";
import { open } from "@tauri-apps/plugin-shell";
import type { AuthChangeEvent, Session } from "@supabase/supabase-js";

import { launcherAuth, parseMinLauncherUrl } from "../auth";
import { Button } from "./Button";

type Provider = "google" | "discord";

export function MinLauncherAccount() {
  const [session, setSession] = useState<Session | null>(null);
  const [busy, setBusy] = useState(false);
  const [waitingForBrowser, setWaitingForBrowser] = useState(false);
  const [error, setError] = useState("");
  const processedCodes = useRef(new Set<string>());

  useEffect(() => {
    const configuredAuth = launcherAuth;
    if (!configuredAuth || !isTauri()) return;
    const auth = configuredAuth as NonNullable<typeof launcherAuth>;
    let active = true;
    const { data: { subscription } } = auth.auth.onAuthStateChange(
      (_event: AuthChangeEvent, nextSession) => {
        if (active) setSession(nextSession);
      },
    );

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
        } else {
          const { data, error: handoffError } = await auth.functions.invoke("redeem-handoff", {
            body: { code: action.code },
          });
          if (handoffError) throw handoffError;
          if (typeof data?.token_hash !== "string") throw new Error("Не удалось подтвердить вход с сайта.");
          const { error: verifyError } = await auth.auth.verifyOtp({
            type: "magiclink",
            token_hash: data.token_hash,
          });
          if (verifyError) throw verifyError;
        }
      } catch (cause) {
        processedCodes.current.delete(action.code);
        setError(cause instanceof Error ? cause.message : "Не удалось войти в аккаунт MinLauncher.");
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
          setError(cause instanceof Error ? cause.message : "Не удалось получить возврат из браузера.");
          setWaitingForBrowser(false);
        }
      }
    })();

    void auth.auth.getSession().then(({ data, error: sessionError }) => {
      if (active) {
        setSession(data.session);
        if (sessionError) setError(sessionError.message);
      }
    }).catch((cause: unknown) => {
      if (active) setError(cause instanceof Error ? cause.message : "Не удалось прочитать сессию.");
    });

    return () => {
      active = false;
      stopListening?.();
      subscription.unsubscribe();
    };
  }, []);

  async function signIn(provider: Provider) {
    if (!launcherAuth) {
      setError("Добавь VITE_SUPABASE_URL и VITE_SUPABASE_ANON_KEY в apps/ui/.env.local.");
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
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Не удалось выйти из аккаунта.");
    } finally {
      setBusy(false);
    }
  }

  const displayName = String(
    session?.user.user_metadata.display_name ??
      session?.user.user_metadata.full_name ??
      session?.user.email ??
      "Игрок MinLauncher",
  );

  return (
    <section className="card minlauncher-account" aria-labelledby="minlauncher-account-title">
      <div className="card__title" id="minlauncher-account-title">Аккаунт MinLauncher</div>
      {!isTauri() ? (
        <p className="field__hint">Вход доступен в установленном приложении MinLauncher.</p>
      ) : session ? (
        <div className="minlauncher-account__profile">
          <span className="accounts__avatar" aria-hidden="true">{displayName.slice(0, 1).toUpperCase()}</span>
          <span><strong>{displayName}</strong><small>{session.user.email ?? "Почта не предоставлена"}</small></span>
          <Button variant="ghost" disabled={busy} onClick={() => void signOut()}>
            {busy ? "Выходим…" : "Выйти"}
          </Button>
        </div>
      ) : (
        <>
          <p className="field__hint">Первый вход создаст аккаунт. Он синхронизируется с сайтом и остаётся отдельным от Minecraft-профиля.</p>
          <div className="minlauncher-account__providers">
            <Button busy={busy} disabled={waitingForBrowser} onClick={() => void signIn("google")}>Войти через Google</Button>
            <Button variant="ghost" busy={busy} disabled={waitingForBrowser} onClick={() => void signIn("discord")}>Войти через Discord</Button>
          </div>
        </>
      )}
      {waitingForBrowser ? <p className="field__hint" role="status">Ожидаем подтверждение в браузере… <button className="minlauncher-account__cancel" type="button" onClick={() => setWaitingForBrowser(false)}>Отменить</button></p> : null}
      {busy && !session ? <p className="field__hint" role="status">Подготавливаем защищённый вход…</p> : null}
      {error ? <p className="field__error" role="alert">{error}</p> : null}
    </section>
  );
}
