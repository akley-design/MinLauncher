import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type FormEvent,
} from "react";
import type { Session } from "@supabase/supabase-js";
import { AuthPage, PasswordRecoveryPage } from "./account-auth";
import { nicknamePattern, nicknamePatternSource } from "./auth-form";
import { supabase } from "./supabase";
import { ResponsiveImage, siteMedia } from "./media";
import { Icon, PrimaryLink } from "./ui";

function NicknameForm({
  initial = "",
  onSaved,
  submitLabel = "Сохранить ник",
}: {
  initial?: string;
  onSaved: (profile: { nickname: string; player_uuid: string }) => void;
  submitLabel?: string;
}) {
  const [nickname, setNickname] = useState(initial);
  const [available, setAvailable] = useState<boolean | null>(null);
  const [checking, setChecking] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const valid = nicknamePattern.test(nickname);

  useEffect(() => {
    setNickname(initial);
  }, [initial]);

  useEffect(() => {
    setAvailable(null);
    const auth = supabase;
    if (!auth || !valid) return;
    let active = true;
    const timer = window.setTimeout(() => {
      setChecking(true);
      void auth.functions
        .invoke("launcher-nickname", {
          body: { operation: "check", nickname },
        })
        .then(({ data, error: checkError }) => {
          if (!active) return;
          if (checkError) setError(checkError.message);
          else setAvailable(data?.available === true);
        })
        .finally(() => {
          if (active) setChecking(false);
        });
    }, 350);
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [nickname, valid]);

  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!supabase || !valid || available !== true) return;
    setBusy(true);
    setError("");
    const { data, error: saveError } = await supabase.functions.invoke(
      "launcher-nickname",
      {
        body: { operation: "claim_profile", nickname },
      },
    );
    setBusy(false);
    if (saveError) setError(saveError.message);
    else if (data?.profile?.nickname && data?.profile?.player_uuid)
      onSaved(data.profile);
    else setError("Сервер не подтвердил ник.");
  }

  return (
    <form className="settings-form" onSubmit={(event) => void save(event)}>
      <label className="field">
        <span>НИК MINLAUNCHER</span>
        <input
          maxLength={16}
          value={nickname}
          onChange={(event) => setNickname(event.target.value)}
          placeholder="Например, Steve_1"
          autoComplete="nickname"
        />
      </label>
      <small role="status">
        {!valid
          ? "3–16 латинских букв, цифр или _"
          : checking
            ? "Проверяем…"
            : available === true
              ? "Ник свободен"
              : available === false
                ? "Ник уже занят"
                : ""}
      </small>
      <small>
        Ник проверяется в реестре MinLauncher и не связан с проверкой аккаунтов
        Minecraft.
      </small>
      <button
        className="button button--primary"
        type="submit"
        disabled={busy || checking || available !== true}
      >
        {busy ? "СОХРАНЕНИЕ…" : submitLabel}
        <Icon name="check" />
      </button>
      {error && (
        <p className="form-message form-message--error" role="alert">
          {error}
        </p>
      )}
    </form>
  );
}

function AccountPage({
  session,
  authLoading,
}: {
  session: Session | null;
  authLoading: boolean;
}) {
  const [name, setName] = useState("");
  const [channel, setChannel] = useState("stable");
  const [registeredNickname, setRegisteredNickname] = useState("");
  const [nicknameAvailable, setNicknameAvailable] = useState<boolean | null>(
    null,
  );
  const [checkingNickname, setCheckingNickname] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [minecraftLinked, setMinecraftLinked] = useState<{
    uuid: string;
    username: string;
    linked_at: string;
  } | null>(null);
  const [minecraftBusy, setMinecraftBusy] = useState(false);
  const [minecraftHandoffUrl, setMinecraftHandoffUrl] = useState("");
  const [minecraftError, setMinecraftError] = useState("");
  const metadata = session?.user.user_metadata ?? {};

  useEffect(() => {
    setName(String(metadata.display_name ?? metadata.full_name ?? ""));
    setChannel(String(metadata.release_channel ?? "stable"));
    if (supabase && session) {
      void supabase.functions
        .invoke("launcher-nickname", { body: { operation: "get_profile" } })
        .then(({ data }) => {
          if (data?.profile?.nickname) {
            setRegisteredNickname(data.profile.nickname);
            setName(data.profile.nickname);
          }
        });
    }
  }, [
    session?.user.id,
    metadata.display_name,
    metadata.full_name,
    metadata.release_channel,
  ]);

  useEffect(() => {
    if (!supabase || !session) return;
    let active = true;
    void supabase.functions
      .invoke("minecraft-account-status")
      .then(({ data, error: statusError }) => {
        if (active && !statusError) setMinecraftLinked(data?.linked ?? null);
      });
    const params = new URLSearchParams(window.location.search);
    const handoffCode = params.get("minecraft_handoff");
    const oauthError = params.get("minecraft_error");
    if (!handoffCode && !oauthError)
      return () => {
        active = false;
      };
    window.history.replaceState(
      {},
      "",
      `${window.location.pathname}${window.location.hash}`,
    );
    if (oauthError) {
      const errors: Record<string, string> = {
        cancelled: "Вход Microsoft отменён.",
        already_linked_elsewhere:
          "Этот Microsoft-аккаунт уже привязан к другому профилю MinLauncher.",
        another_account_linked:
          "К профилю уже привязан другой Microsoft-аккаунт. Сначала отвяжите его.",
        minecraft_unavailable:
          "У аккаунта нет Minecraft Java Edition или Xbox-профиля, либо Microsoft не разрешил вход.",
        not_configured:
          "Вход Microsoft ещё не настроен на сервере MinLauncher.",
        expired: "Срок входа Microsoft истёк. Начните привязку заново.",
      };
      setMinecraftError(
        errors[oauthError] ?? "Microsoft не завершил вход. Попробуйте снова.",
      );
      return () => {
        active = false;
      };
    }
    if (!handoffCode)
      return () => {
        active = false;
      };
    setMinecraftBusy(true);
    setMinecraftError("");
    const handoff = `minlauncher://auth/minecraft?code=${encodeURIComponent(handoffCode)}`;
    setMinecraftHandoffUrl(handoff);
    window.location.href = handoff;
    setMinecraftBusy(false);
    return () => {
      active = false;
    };
  }, [session?.user.id]);

  async function connectMinecraft() {
    if (!supabase) return;
    setMinecraftBusy(true);
    setMinecraftError("");
    setMinecraftHandoffUrl("");
    try {
      const { data, error: startError } = await supabase.functions.invoke(
        "minecraft-oauth-start",
      );
      if (startError) throw startError;
      if (typeof data?.authorize_url !== "string")
        throw new Error("Microsoft-вход не настроен на сервере.");
      window.location.assign(data.authorize_url);
    } catch (cause) {
      setMinecraftError(
        cause instanceof Error
          ? cause.message
          : "Не удалось начать вход Microsoft.",
      );
      setMinecraftBusy(false);
    }
  }

  async function unlinkMinecraft() {
    if (!supabase) return;
    setMinecraftBusy(true);
    setMinecraftError("");
    try {
      const { error: unlinkError } = await supabase.functions.invoke(
        "minecraft-account-unlink",
      );
      if (unlinkError) throw unlinkError;
      setMinecraftLinked(null);
      setMinecraftHandoffUrl("");
    } catch (cause) {
      setMinecraftError(
        cause instanceof Error
          ? cause.message
          : "Не удалось отвязать Minecraft.",
      );
    } finally {
      setMinecraftBusy(false);
    }
  }

  useEffect(() => {
    const valid = nicknamePattern.test(name);
    setNicknameAvailable(null);
    if (!valid) return;
    if (
      registeredNickname &&
      name.toLowerCase() === registeredNickname.toLowerCase()
    ) {
      setNicknameAvailable(true);
      return;
    }
    const auth = supabase;
    if (!auth) return;
    let active = true;
    const timer = window.setTimeout(() => {
      setCheckingNickname(true);
      void auth.functions
        .invoke("launcher-nickname", {
          body: { operation: "check", nickname: name },
        })
        .then(({ data, error: checkError }) => {
          if (active && !checkError)
            setNicknameAvailable(data?.available === true);
        })
        .finally(() => {
          if (active) setCheckingNickname(false);
        });
    }, 350);
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [name, registeredNickname]);

  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!supabase || !session) return;
    setBusy(true);
    setMessage("");
    setError("");
    try {
      const { data: result, error: profileError } =
        await supabase.functions.invoke("launcher-nickname", {
          body: { operation: "claim_profile", nickname: name.trim() },
        });
      if (profileError) throw profileError;
      if (!result?.profile?.nickname) throw new Error("Ник не сохранён.");
      const { error: saveError } = await supabase.auth.updateUser({
        data: {
          ...metadata,
          display_name: result.profile.nickname,
          release_channel: channel,
        },
      });
      if (saveError) setError(saveError.message);
      else setMessage("Настройки профиля сохранены.");
    } catch (saveError) {
      setError(
        saveError instanceof Error
          ? saveError.message
          : "Не удалось сохранить настройки.",
      );
    } finally {
      setBusy(false);
    }
  }

  async function signOut() {
    if (!supabase) return;
    setBusy(true);
    try {
      const { error: signOutError } = await supabase.auth.signOut();
      if (signOutError) setError(signOutError.message);
      else window.location.assign("/");
    } catch (signOutError) {
      setError(
        signOutError instanceof Error
          ? signOutError.message
          : "Не удалось выйти из аккаунта.",
      );
    } finally {
      setBusy(false);
    }
  }

  if (authLoading)
    return (
      <section className="page settings-page">
        <div className="settings-banner section-wrap">
          <div className="settings-banner__copy">
            <div className="page-kicker">
              <span className="eyebrow">ПРОВЕРЯЕМ АККАУНТ…</span>
              <span className="page-index">ACCOUNT / SETTINGS</span>
            </div>
          </div>
        </div>
      </section>
    );
  if (!session)
    return (
      <section className="page settings-page settings-page--empty">
        <div className="settings-banner section-wrap">
          <div className="settings-banner__art" aria-hidden="true">
            <ResponsiveImage image={siteMedia.wide.island} alt="" />
          </div>
          <div className="settings-banner__copy">
            <div className="page-kicker">
              <span className="eyebrow">
                <i /> Личный профиль
              </span>
              <span className="page-index">ACCOUNT / SETTINGS</span>
            </div>
            <h1 className="page-display">
              Профиль
              <br />
              <em>MinLauncher.</em>
            </h1>
            <p className="page-lede">
              Войди, чтобы изменить имя и канал обновлений.
            </p>
            <PrimaryLink href="/account">Войти в аккаунт</PrimaryLink>
          </div>
        </div>
      </section>
    );

  return (
    <section className="page settings-page">
      <div className="settings-banner section-wrap">
        <div className="settings-banner__art" aria-hidden="true">
          <ResponsiveImage image={siteMedia.wide.island} alt="" />
        </div>
        <div className="settings-banner__copy">
          <div className="page-kicker">
            <span className="eyebrow">
              <i /> Личный профиль
            </span>
            <span className="page-index">ACCOUNT / SETTINGS</span>
          </div>
          <h1 className="page-display">
            Настройки
            <br />
            <em>профиля.</em>
          </h1>
          <p className="settings-banner__email">{session.user.email}</p>
        </div>
      </div>
      <div className="settings-layout section-wrap">
        <aside className="settings-side">
          <div className="account-avatar">
            {(name || session.user.email || "M").slice(0, 1).toUpperCase()}
          </div>
          <div>
            <span className="eyebrow">MINLAUNCHER ACCOUNT</span>
            <p className="settings-side__name">{name || session.user.email}</p>
          </div>
          <dl className="settings-side__meta">
            <div>
              <dt>Почта</dt>
              <dd>{session.user.email}</dd>
            </div>
            <div>
              <dt>Канал</dt>
              <dd>{channel}</dd>
            </div>
            <div>
              <dt>Minecraft</dt>
              <dd>
                {minecraftLinked ? minecraftLinked.username : "не привязан"}
              </dd>
            </div>
          </dl>
        </aside>
        <form className="settings-form" onSubmit={(event) => void save(event)}>
          <label className="field">
            <span>НИК MINLAUNCHER</span>
            <input
              maxLength={16}
              pattern={nicknamePatternSource}
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="Например, Steve_1"
              autoComplete="nickname"
            />
            <small role="status">
              {!nicknamePattern.test(name)
                ? "3–16 латинских букв, цифр или _"
                : checkingNickname
                  ? "Проверяем…"
                  : nicknameAvailable === true
                    ? "Ник свободен"
                    : nicknameAvailable === false
                      ? "Ник уже занят"
                      : ""}
            </small>
            <small>
              Занятость проверяется среди профилей MinLauncher, не в Minecraft.
            </small>
          </label>
          <label className="field">
            <span>КАНАЛ ОБНОВЛЕНИЙ</span>
            <select
              value={channel}
              onChange={(event) => setChannel(event.target.value)}
            >
              <option value="stable">Стабильный</option>
              <option value="preview">Предварительный доступ</option>
            </select>
          </label>
          <div className="field field--email">
            <span>АККАУНТ</span>
            <p>{session.user.email}</p>
            <small>
              Почта используется для подтверждения аккаунта и восстановления
              доступа.
            </small>
          </div>
          <div className="field field--email">
            <span>MINECRAFT JAVA</span>
            {minecraftLinked ? (
              <>
                <p>Подключён профиль {minecraftLinked.username}</p>
                <small>UUID: {minecraftLinked.uuid}</small>
                <button
                  className="button button--ghost"
                  type="button"
                  disabled={minecraftBusy}
                  onClick={() => void connectMinecraft()}
                >
                  {minecraftBusy
                    ? "ПОДГОТАВЛИВАЕМ ВХОД…"
                    : "Обновить вход Microsoft"}
                </button>
                <button
                  className="button button--ghost"
                  type="button"
                  disabled={minecraftBusy}
                  onClick={() => void unlinkMinecraft()}
                >
                  {minecraftBusy ? "ОБНОВЛЯЕМ…" : "Отвязать Minecraft"}
                </button>
              </>
            ) : (
              <>
                <p>Подключите Microsoft-аккаунт с Minecraft Java Edition.</p>
                <button
                  className="button button--ghost"
                  type="button"
                  disabled={minecraftBusy}
                  onClick={() => void connectMinecraft()}
                >
                  {minecraftBusy
                    ? "ПОДГОТАВЛИВАЕМ ВХОД…"
                    : "Привязать Microsoft / Minecraft"}
                </button>
              </>
            )}
            {minecraftHandoffUrl ? (
              <a className="button button--primary" href={minecraftHandoffUrl}>
                Открыть MinLauncher
              </a>
            ) : null}
            {minecraftError ? (
              <small className="form-message form-message--error" role="alert">
                {minecraftError}
              </small>
            ) : null}
            <small>
              Откроется официальный вход Microsoft. MinLauncher не просит пароль
              Microsoft; игровая сессия передаётся лаунчеру по одноразовой
              ссылке.
            </small>
          </div>
          <div className="settings-actions">
            <button
              className="button button--primary"
              type="submit"
              disabled={busy || !supabase || nicknameAvailable !== true}
            >
              {busy ? "СОХРАНЕНИЕ…" : "Сохранить настройки"}
              <Icon name="check" />
            </button>
            <button
              className="button button--ghost"
              type="button"
              onClick={() => void signOut()}
              disabled={busy}
            >
              Выйти из аккаунта
            </button>
          </div>
          {message && (
            <p className="form-message form-message--success" role="status">
              {message}
            </p>
          )}
          {error && (
            <p className="form-message form-message--error" role="alert">
              {error}
            </p>
          )}
        </form>
      </div>
      <p className="account-disclaimer section-wrap">
        Канал обновлений сохраняется в профиле сайта. Он пока не меняет
        настройки установленного приложения.
      </p>
    </section>
  );
}

function CallbackPage({
  authLoading,
  session,
}: {
  authLoading: boolean;
  session: Session | null;
}) {
  const [error, setError] = useState("");
  const [handoffUrl, setHandoffUrl] = useState("");
  const [handoffBusy, setHandoffBusy] = useState(false);
  const [handoffError, setHandoffError] = useState("");
  const [needsNickname, setNeedsNickname] = useState(false);
  const [profileLoaded, setProfileLoaded] = useState(false);
  const startedFor = useRef("");

  const createHandoff = useCallback(async () => {
    if (!supabase || !session) return;
    setHandoffBusy(true);
    setHandoffError("");
    try {
      const { data, error: functionError } =
        await supabase.functions.invoke("create-handoff");
      if (functionError) throw functionError;
      if (typeof data?.code !== "string")
        throw new Error("Сервер не выдал ссылку для входа.");
      const url = `minlauncher://auth/transfer?code=${encodeURIComponent(data.code)}`;
      setHandoffUrl(url);
      window.location.href = url;
    } catch (cause) {
      setHandoffError(
        cause instanceof Error
          ? cause.message
          : "Не удалось подготовить вход в лаунчер.",
      );
    } finally {
      setHandoffBusy(false);
    }
  }, [session]);

  useEffect(() => {
    if (
      window.sessionStorage.getItem("minlauncher:password-recovery") === "1"
    ) {
      setProfileLoaded(true);
      return;
    }
    const params = new URLSearchParams(window.location.search);
    const hash = new URLSearchParams(window.location.hash.replace(/^#/, ""));
    setError(
      params.get("error_description") ?? hash.get("error_description") ?? "",
    );
    if (!authLoading && session && startedFor.current !== session.user.id) {
      startedFor.current = session.user.id;
      void (async () => {
        const { data, error: profileError } = await supabase!.functions.invoke(
          "launcher-nickname",
          {
            body: { operation: "get_profile" },
          },
        );
        if (profileError) setHandoffError(profileError.message);
        else if (data?.profile) void createHandoff();
        else {
          const requestedNickname =
            session.user.user_metadata.requested_nickname;
          if (typeof requestedNickname === "string") {
            const { data: claim, error: claimError } =
              await supabase!.functions.invoke("launcher-nickname", {
                body: {
                  operation: "claim_profile",
                  nickname: requestedNickname,
                },
              });
            if (!claimError && claim?.profile) void createHandoff();
            else {
              setHandoffError(
                "Этот ник уже занят. Выберите другой ник для профиля.",
              );
              setNeedsNickname(true);
            }
          } else setNeedsNickname(true);
        }
        setProfileLoaded(true);
      })();
    }
  }, [authLoading, session?.user.id, createHandoff]);

  return (
    <section className="auth-page auth-page--airlock">
      <div className="auth-wash" aria-hidden="true">
        <ResponsiveImage image={siteMedia.wide.portal} alt="" lazy />
      </div>
      <div className="auth-card">
        <span className="eyebrow">MINLAUNCHER ACCOUNT</span>
        <h1>
          {error ? (
            <>
              ВХОД
              <br />
              <em>НЕ УДАЛСЯ.</em>
            </>
          ) : session ? (
            <>
              АККАУНТ
              <br />
              <em>ГОТОВ.</em>
            </>
          ) : (
            <>
              СЕССИЯ
              <br />
              <em>НЕ НАЙДЕНА.</em>
            </>
          )}
        </h1>
        <p>
          {error ||
            (authLoading
              ? "Проверяем защищённый вход…"
              : session && needsNickname
                ? "Выбери ник MinLauncher. После сохранения профиль передастся в лаунчер."
                : session && !profileLoaded
                  ? "Проверяем профиль…"
                  : session
                    ? "Открываем MinLauncher. Ссылка действует одну минуту и используется один раз."
                    : "Сессия не найдена. Попробуй войти ещё раз.")}
        </p>
        {session && needsNickname ? (
          <NicknameForm
            onSaved={() => {
              setHandoffError("");
              setNeedsNickname(false);
              void createHandoff();
            }}
          />
        ) : null}
        {handoffUrl && (
          <a className="button button--primary" href={handoffUrl}>
            Открыть MinLauncher <Icon name="arrow" />
          </a>
        )}
        {!authLoading && !session && (
          <PrimaryLink href="/account">Вернуться ко входу</PrimaryLink>
        )}
        {session && (
          <button
            className="button button--ghost"
            type="button"
            disabled={handoffBusy}
            onClick={() => void createHandoff()}
          >
            {handoffBusy
              ? "ГОТОВИМ ССЫЛКУ…"
              : "ОБНОВИТЬ ССЫЛКУ И ОТКРЫТЬ ЛАУНЧЕР"}
            <Icon name="arrow" />
          </button>
        )}
        {handoffError && (
          <p className="form-message form-message--error" role="alert">
            {handoffError}
          </p>
        )}
      </div>
    </section>
  );
}
export { AccountPage, AuthPage, CallbackPage, PasswordRecoveryPage };
