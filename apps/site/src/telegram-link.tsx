import { useCallback, useEffect, useMemo, useState } from "react";
import type { SupabaseClient } from "@supabase/supabase-js";

type LinkStatus = "loading" | "signed_out" | "unlinked" | "linked" | "unavailable";
type Action = "create" | "refresh" | "unlink";
type LinkChallenge = { href: string; expiresAt: string };
type Language = "ru" | "uk";

type Copy = {
  title: string;
  loading: string;
  signedOut: string;
  unlinked: string;
  linked: string;
  unavailable: string;
  intro: string;
  create: string;
  createAgain: string;
  openTelegram: string;
  expires: string;
  expired: string;
  refresh: string;
  unlink: string;
  confirmUnlink: string;
  cancel: string;
  unlinkedSuccess: string;
  alreadyLinked: string;
  linkExpired: string;
  linkUsed: string;
  botMissing: string;
  apiUnavailable: string;
  genericError: string;
  privacy: string;
};

const COPY: Record<Language, Copy> = {
  ru: {
    title: "TELEGRAM",
    loading: "Проверяем привязку…",
    signedOut: "Войдите в аккаунт MinLauncher, чтобы привязать Telegram.",
    unlinked: "Профиль не привязан",
    linked: "Профиль привязан",
    unavailable: "Статус привязки недоступен",
    intro: "Свяжите аккаунты, чтобы открыть в боте уровень и косметические награды лаунчера.",
    create: "Создать ссылку",
    createAgain: "Создать новую ссылку",
    openTelegram: "Открыть Telegram",
    expires: "Ссылка действует 10 минут. После запуска бота вернитесь сюда и проверьте привязку.",
    expired: "Срок ссылки истёк. Создайте новую, чтобы продолжить.",
    refresh: "Проверить привязку",
    unlink: "Отвязать аккаунт",
    confirmUnlink: "Подтвердить отвязку",
    cancel: "Отмена",
    unlinkedSuccess: "Telegram-аккаунт отвязан. Прогресс лаунчера сохранён.",
    alreadyLinked: "Этот профиль или Telegram-аккаунт уже привязан.",
    linkExpired: "Ссылка истекла. Создайте новую и попробуйте ещё раз.",
    linkUsed: "Ссылка уже использована. Создайте новую, если привязка не появилась.",
    botMissing: "Привязка Telegram ещё не настроена на сайте.",
    apiUnavailable: "Не удалось связаться с сервисом привязки. Попробуйте позже.",
    genericError: "Не удалось выполнить действие. Попробуйте ещё раз.",
    privacy: "Отвязка удаляет только связь аккаунтов; аккаунт и игровой прогресс MinLauncher остаются.",
  },
  uk: {
    title: "TELEGRAM",
    loading: "Перевіряємо прив’язку…",
    signedOut: "Увійдіть в акаунт MinLauncher, щоб прив’язати Telegram.",
    unlinked: "Профіль не прив’язано",
    linked: "Профіль прив’язано",
    unavailable: "Статус прив’язки недоступний",
    intro: "Зв’яжіть акаунти, щоб відкрити в боті рівень і косметичні нагороди лаунчера.",
    create: "Створити посилання",
    createAgain: "Створити нове посилання",
    openTelegram: "Відкрити Telegram",
    expires: "Посилання діє 10 хвилин. Після запуску бота поверніться сюди та перевірте прив’язку.",
    expired: "Термін дії посилання минув. Створіть нове, щоб продовжити.",
    refresh: "Перевірити прив’язку",
    unlink: "Відв’язати акаунт",
    confirmUnlink: "Підтвердити відв’язування",
    cancel: "Скасувати",
    unlinkedSuccess: "Telegram-акаунт відв’язано. Прогрес лаунчера збережено.",
    alreadyLinked: "Цей профіль або Telegram-акаунт уже прив’язано.",
    linkExpired: "Посилання прострочене. Створіть нове й спробуйте ще раз.",
    linkUsed: "Посилання вже використано. Створіть нове, якщо прив’язка не з’явилася.",
    botMissing: "Прив’язку Telegram ще не налаштовано на сайті.",
    apiUnavailable: "Не вдалося зв’язатися із сервісом прив’язки. Спробуйте пізніше.",
    genericError: "Не вдалося виконати дію. Спробуйте ще раз.",
    privacy: "Відв’язування видаляє лише зв’язок акаунтів; акаунт і ігровий прогрес MinLauncher залишаються.",
  },
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function apiCode(data: unknown): string | null {
  return isRecord(data) && typeof data.error === "string" ? data.error : null;
}

async function responseErrorCode(data: unknown, error: unknown): Promise<string | null> {
  const bodyCode = apiCode(data);
  if (bodyCode) return bodyCode;
  if (!isRecord(error) || !isRecord(error.context) || typeof error.context.json !== "function") {
    return null;
  }
  try {
    const readJson = error.context.json as () => Promise<unknown>;
    return apiCode(await readJson.call(error.context));
  } catch {
    // Some gateway failures do not include a JSON response body.
    return null;
  }
}

function isValidUsername(value: string): boolean {
  return /^[A-Za-z0-9_]{5,32}$/.test(value);
}

function messageForCode(code: string | null, copy: Copy): string {
  if (code === "already_linked") return copy.alreadyLinked;
  if (code === "link_expired") return copy.linkExpired;
  if (code === "link_used") return copy.linkUsed;
  if (code === "service_unavailable") return copy.apiUnavailable;
  return copy.genericError;
}

export function TelegramLinkCard({
  client,
  userId,
  signedIn,
  botUsername,
  language,
}: {
  client: Pick<SupabaseClient, "functions"> | null;
  userId: string | null;
  signedIn: boolean;
  botUsername?: string;
  language?: Language;
}) {
  const detectedLanguage: Language = language ??
    (typeof navigator !== "undefined" && navigator.language.toLowerCase().startsWith("uk")
      ? "uk"
      : "ru");
  const copy = COPY[detectedLanguage];
  const username = (botUsername ?? "").trim().replace(/^@/, "");
  const usernameValid = isValidUsername(username);
  const [status, setStatus] = useState<LinkStatus>(signedIn ? "loading" : "signed_out");
  const [busy, setBusy] = useState<Action | null>(null);
  const [challenge, setChallenge] = useState<LinkChallenge | null>(null);
  const [confirmingUnlink, setConfirmingUnlink] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [currentTime, setCurrentTime] = useState(() => Date.now());

  const challengeExpired = useMemo(
    () => challenge !== null && Date.parse(challenge.expiresAt) <= currentTime,
    [challenge, currentTime],
  );
  const challengeInvalid = useMemo(
    () => challenge !== null && !Number.isFinite(Date.parse(challenge.expiresAt)),
    [challenge],
  );

  const refreshStatus = useCallback(async () => {
    if (!client || !signedIn) {
      setStatus(signedIn ? "unavailable" : "signed_out");
      return;
    }
    setBusy("refresh");
    setError("");
    try {
      const { data, error: invokeError } = await client.functions.invoke("telegram-link", {
        body: { action: "status" },
      });
      if (invokeError || !isRecord(data) || typeof data.linked !== "boolean") {
        throw new Error("status_unavailable");
      }
      setStatus(data.linked ? "linked" : "unlinked");
      if (data.linked) setChallenge(null);
    } catch {
      setStatus((current) => current === "linked" ? current : "unavailable");
      setError(copy.apiUnavailable);
    } finally {
      setBusy(null);
    }
  }, [client, signedIn, copy.apiUnavailable]);

  useEffect(() => {
    setChallenge(null);
    setMessage("");
    setError("");
    setConfirmingUnlink(false);
    setStatus(signedIn ? "loading" : "signed_out");
    if (signedIn) void refreshStatus();
  }, [userId, signedIn, refreshStatus]);

  useEffect(() => {
    if (!challenge || challengeInvalid || challengeExpired) return;
    const expiry = Date.parse(challenge.expiresAt);
    const timer = window.setTimeout(
      () => setCurrentTime(Date.now()),
      Math.min(2_147_483_647, Math.max(0, expiry - Date.now())),
    );
    return () => window.clearTimeout(timer);
  }, [challenge, challengeInvalid, challengeExpired]);

  async function createLink() {
    if (!client || !signedIn || !usernameValid) return;
    setBusy("create");
    setError("");
    setMessage("");
    try {
      const { data, error: invokeError } = await client.functions.invoke("telegram-link", {
        body: { action: "create" },
      });
      if (invokeError) {
        const code = await responseErrorCode(data, invokeError);
        if (code === "already_linked") setStatus("linked");
        throw new Error(code ?? "request_failed");
      }
      if (
        !isRecord(data) ||
        typeof data.payload !== "string" ||
        !/^[A-Za-z0-9_-]{43}$/.test(data.payload) ||
        typeof data.expiresAt !== "string" ||
        !Number.isFinite(Date.parse(data.expiresAt))
      ) {
        throw new Error("invalid_response");
      }
      setCurrentTime(Date.now());
      setChallenge({
        href: `https://t.me/${username}?start=link_${data.payload}`,
        expiresAt: data.expiresAt,
      });
    } catch (cause) {
      const code = cause instanceof Error ? cause.message : null;
      setError(code === "request_failed" ? copy.apiUnavailable : messageForCode(code, copy));
    } finally {
      setBusy(null);
    }
  }

  async function unlink() {
    if (!client || !signedIn) return;
    setBusy("unlink");
    setError("");
    setMessage("");
    try {
      const { data, error: invokeError } = await client.functions.invoke("telegram-link", {
        body: { action: "unlink" },
      });
      if (invokeError || (isRecord(data) && typeof data.error === "string")) {
        throw new Error((await responseErrorCode(data, invokeError)) ?? "request_failed");
      }
      setStatus("unlinked");
      setChallenge(null);
      setConfirmingUnlink(false);
      setMessage(copy.unlinkedSuccess);
    } catch (cause) {
      const code = cause instanceof Error ? cause.message : null;
      setError(messageForCode(code, copy));
    } finally {
      setBusy(null);
    }
  }

  const statusText = {
    loading: copy.loading,
    signed_out: copy.signedOut,
    unlinked: copy.unlinked,
    linked: copy.linked,
    unavailable: copy.unavailable,
  }[status];

  return (
    <section className="field field--email telegram-link-card" aria-labelledby="telegram-link-title">
      <span id="telegram-link-title">{copy.title}</span>
      <p className="telegram-link-card__status" role="status" aria-live="polite">
        {statusText}
      </p>
      <small>{copy.intro}</small>
      {!signedIn ? null : status === "linked" ? (
        <>
          {confirmingUnlink ? (
            <div className="telegram-link-card__actions">
              <button
                className="button button--primary"
                type="button"
                disabled={busy !== null}
                onClick={() => void unlink()}
              >
                {busy === "unlink" ? "…" : copy.confirmUnlink}
              </button>
              <button
                className="button button--ghost"
                type="button"
                disabled={busy !== null}
                onClick={() => setConfirmingUnlink(false)}
              >
                {copy.cancel}
              </button>
            </div>
          ) : (
            <button
              className="button button--ghost"
              type="button"
              disabled={busy !== null}
              onClick={() => setConfirmingUnlink(true)}
            >
              {copy.unlink}
            </button>
          )}
          <small>{copy.privacy}</small>
        </>
      ) : (
        <>
          {status === "unlinked" ? (
            <div className="telegram-link-card__actions">
              <button
                className="button button--primary"
                type="button"
                disabled={busy !== null || !usernameValid}
                onClick={() => void createLink()}
              >
                {busy === "create" ? "…" : challenge ? copy.createAgain : copy.create}
              </button>
              <button
                className="button button--ghost"
                type="button"
                disabled={busy !== null}
                onClick={() => void refreshStatus()}
              >
                {copy.refresh}
              </button>
            </div>
          ) : status === "unavailable" ? (
            <button
              className="button button--ghost"
              type="button"
              disabled={busy !== null}
              onClick={() => void refreshStatus()}
            >
              {copy.refresh}
            </button>
          ) : null}
          {challenge && !challengeExpired && !challengeInvalid ? (
            <div className="telegram-link-card__challenge">
              <a
                className="button button--primary"
                href={challenge.href}
                target="_blank"
                rel="noreferrer"
                referrerPolicy="no-referrer"
              >
                {copy.openTelegram}
              </a>
              <small>{copy.expires}</small>
            </div>
          ) : challengeExpired || challengeInvalid ? (
            <small role="status">{copy.expired}</small>
          ) : null}
        </>
      )}
      {!usernameValid && signedIn ? <small role="alert">{copy.botMissing}</small> : null}
      {message ? <small className="form-message form-message--success" role="status">{message}</small> : null}
      {error ? <small className="form-message form-message--error" role="alert">{error}</small> : null}
    </section>
  );
}
