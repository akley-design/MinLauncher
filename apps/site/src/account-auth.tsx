import { useEffect, useState, type FormEvent, type ReactNode } from "react";
import type { Session } from "@supabase/supabase-js";
import { ResponsiveImage, siteMedia } from "./media";
import { supabase } from "./supabase";
import {
  nicknamePatternSource,
  validateNewPassword,
  validateRegistration,
} from "./auth-form";

type AuthMode = "login" | "register" | "recover";

function emailErrorMessage(error: unknown, fallback: string) {
  if (!error || typeof error !== "object") return fallback;

  const code = "code" in error ? error.code : undefined;
  const status = "status" in error ? error.status : undefined;
  const message =
    "message" in error && typeof error.message === "string"
      ? error.message
      : "";
  if (status === 429 || code === "over_email_send_rate_limit") {
    return "Слишком много попыток отправки. Подождите немного и попробуйте снова.";
  }
  if (code === "email_address_not_authorized") {
    return "Отправка на этот адрес пока не настроена. Попробуйте позже.";
  }
  if (
    message.includes("Error sending confirmation email") ||
    message.includes("Error sending recovery email")
  ) {
    return "Не удалось отправить письмо. В Supabase не настроен SMTP-сервер или исчерпан лимит писем.";
  }
  return fallback;
}

function AuthGateway({ children }: { children: ReactNode }) {
  return (
    <section className="auth-page auth-page--gateway">
      <div className="auth-art page-art" aria-hidden="true">
        <ResponsiveImage image={siteMedia.wide.portal} alt="" priority />
        <span className="auth-art__caption">MINLAUNCHER / PORTAL</span>
      </div>
      {children}
    </section>
  );
}

function AuthWash({
  variant,
  children,
}: {
  variant: "cave" | "portal";
  children: ReactNode;
}) {
  return (
    <section
      className={`auth-page auth-page--${variant === "cave" ? "key" : "airlock"}`}
    >
      <div className="auth-wash" aria-hidden="true">
        <ResponsiveImage
          image={
            variant === "cave" ? siteMedia.wide.cave : siteMedia.wide.portal
          }
          alt=""
          priority
        />
      </div>
      {children}
    </section>
  );
}

export function AuthPage({ session }: { session: Session | null }) {
  const [mode, setMode] = useState<AuthMode>("login");
  const [nickname, setNickname] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [repeatPassword, setRepeatPassword] = useState("");
  const [confirmationEmail, setConfirmationEmail] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  function switchMode(nextMode: AuthMode) {
    setMode(nextMode);
    setError("");
    setNotice("");
    setPassword("");
    setRepeatPassword("");
  }

  if (session) {
    return (
      <AuthGateway>
        <div className="auth-card">
          <span className="auth-card__stamp">АККАУНТ АКТИВЕН</span>
          <span className="eyebrow">АККАУНТ MINLAUNCHER</span>
          <h1>
            АККАУНТ
            <br />
            <em>ПОДКЛЮЧЁН.</em>
          </h1>
          <p>
            Вы вошли как {session.user.email ?? "пользователь MinLauncher"}.
          </p>
          <a className="button button--primary" href="/account/settings">
            Настройки профиля
          </a>
        </div>
      </AuthGateway>
    );
  }

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError("");
    setNotice("");
    if (!supabase) {
      setError("Вход временно недоступен. Попробуйте позже.");
      return;
    }
    setBusy(true);
    try {
      if (mode === "register") {
        const validation = validateRegistration({
          nickname,
          email,
          password,
          repeatPassword,
        });
        if (validation) throw new Error(validation);
        const normalizedNick = nickname.trim();
        const { data: check, error: checkError } =
          await supabase.functions.invoke("launcher-nickname", {
            body: { operation: "check", nickname: normalizedNick },
          });
        if (checkError)
          throw new Error("Не удалось проверить ник. Попробуйте позже.");
        if (check?.available !== true) throw new Error("Этот ник уже занят.");

        const normalizedEmail = email.trim().toLowerCase();
        const signupResult = await supabase.auth
          .signUp({
            email: normalizedEmail,
            password,
            options: {
              emailRedirectTo: `${window.location.origin}/auth/callback`,
              data: { requested_nickname: normalizedNick },
            },
          })
          .catch(() => {
            setConfirmationEmail(normalizedEmail);
            setError("Не удалось создать аккаунт. Попробуйте позже.");
            return null;
          });
        if (!signupResult) return;
        const { data, error: signupError } = signupResult;
        if (signupError) {
          setConfirmationEmail(normalizedEmail);
          setError(
            emailErrorMessage(
              signupError,
              "Не удалось создать аккаунт. Попробуйте позже.",
            ),
          );
          return;
        }
        if (data.session) window.location.assign("/auth/callback");
        else {
          if (
            data.user &&
            Array.isArray(data.user.identities) &&
            data.user.identities.length === 0
          ) {
            setError(
              "Пользователь с такой почтой уже зарегистрирован. Войдите или восстановите пароль.",
            );
            return;
          }
          setConfirmationEmail(email.trim().toLowerCase());
          setNotice(
            "Проверьте почту и перейдите по ссылке подтверждения, чтобы завершить регистрацию.",
          );
        }
        return;
      }

      if (mode === "recover") {
        if (!email.trim()) throw new Error("Введите адрес электронной почты.");
        const { error: resetError } = await supabase.auth.resetPasswordForEmail(
          email.trim().toLowerCase(),
          { redirectTo: `${window.location.origin}/auth/callback` },
        );
        if (resetError)
          throw new Error(
            emailErrorMessage(
              resetError,
              "Не удалось отправить письмо. Попробуйте позже.",
            ),
          );
        setNotice(
          "Если адрес привязан к аккаунту, письмо для задания или сброса пароля отправлено.",
        );
        return;
      }

      if (!nickname.trim() || !password)
        throw new Error("Введите ник Minecraft и пароль.");
      const { data, error: signinError } = await supabase.functions.invoke(
        "launcher-nickname",
        { body: { operation: "sign_in", nickname: nickname.trim(), password } },
      );
      if (signinError || !data?.access_token || !data?.refresh_token) {
        throw new Error("Неверный ник или пароль.");
      }
      const { error: sessionError } = await supabase.auth.setSession({
        access_token: data.access_token,
        refresh_token: data.refresh_token,
      });
      if (sessionError) throw new Error("Неверный ник или пароль.");
      window.location.assign("/auth/callback");
    } catch (cause) {
      setError(
        cause instanceof Error ? cause.message : "Не удалось выполнить запрос.",
      );
    } finally {
      setBusy(false);
    }
  }

  async function resendConfirmation() {
    if (!supabase || !confirmationEmail) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const { error: resendError } = await supabase.auth.resend({
        type: "signup",
        email: confirmationEmail,
        options: { emailRedirectTo: `${window.location.origin}/auth/callback` },
      });
      if (resendError) throw resendError;
      setNotice("Если адрес можно подтвердить, письмо отправлено повторно.");
    } catch (cause) {
      setError(
        emailErrorMessage(
          cause,
          "Не удалось отправить письмо. Попробуйте позже.",
        ),
      );
    } finally {
      setBusy(false);
    }
  }

  const isRegister = mode === "register";
  const isRecover = mode === "recover";
  return (
    <AuthGateway>
      <div className="auth-card">
        <a className="back-link" href="/">
          ← На главную
        </a>
        <span className="eyebrow">АККАУНТ MINLAUNCHER</span>
        <h1>
          {isRegister ? (
            <>
              СОЗДАЙ
              <br />
              <em>АККАУНТ.</em>
            </>
          ) : isRecover ? (
            <>
              ЗАДАТЬ
              <br />
              <em>ПАРОЛЬ.</em>
            </>
          ) : (
            <>
              ВОЙТИ
              <br />
              <em>В АККАУНТ.</em>
            </>
          )}
        </h1>
        <p>
          {isRegister
            ? "Создай профиль MinLauncher с подтверждённым email и ником Minecraft."
            : isRecover
              ? "Введи email, чтобы получить защищённую ссылку для задания или сброса пароля."
              : "Войди в MinLauncher по нику Minecraft и паролю."}
        </p>
        {!isRecover && (
          <div className="auth-switch" role="group" aria-label="Режим аккаунта">
            <button
              type="button"
              className={mode === "login" ? "is-active" : ""}
              onClick={() => switchMode("login")}
            >
              Вход
            </button>
            <button
              type="button"
              className={mode === "register" ? "is-active" : ""}
              onClick={() => switchMode("register")}
            >
              Регистрация
            </button>
          </div>
        )}
        <form className="auth-form" onSubmit={(event) => void submit(event)}>
          {isRegister && (
            <label className="field">
              <span>НИК MINECRAFT</span>
              <input
                value={nickname}
                onChange={(event) => setNickname(event.target.value)}
                autoComplete="nickname"
                minLength={3}
                maxLength={16}
                pattern={nicknamePatternSource}
                required
              />
            </label>
          )}
          {(isRegister || isRecover) && (
            <label className="field">
              <span>ЭЛЕКТРОННАЯ ПОЧТА</span>
              <input
                type="email"
                value={email}
                onChange={(event) => setEmail(event.target.value)}
                autoComplete="email"
                required
              />
            </label>
          )}
          {mode === "login" && (
            <label className="field">
              <span>НИК MINECRAFT</span>
              <input
                value={nickname}
                onChange={(event) => setNickname(event.target.value)}
                autoComplete="username"
                maxLength={16}
                required
              />
            </label>
          )}
          {!isRecover && (
            <label className="field">
              <span>ПАРОЛЬ</span>
              <input
                type="password"
                value={password}
                onChange={(event) => setPassword(event.target.value)}
                autoComplete={isRegister ? "new-password" : "current-password"}
                minLength={isRegister ? 12 : undefined}
                maxLength={72}
                required
              />
            </label>
          )}
          {isRegister && (
            <label className="field">
              <span>ПОВТОРИТЕ ПАРОЛЬ</span>
              <input
                type="password"
                value={repeatPassword}
                onChange={(event) => setRepeatPassword(event.target.value)}
                autoComplete="new-password"
                minLength={12}
                maxLength={72}
                required
              />
            </label>
          )}
          <button
            className="button button--primary"
            type="submit"
            disabled={busy}
          >
            {busy
              ? "ПОДОЖДИТЕ…"
              : isRegister
                ? "СОЗДАТЬ АККАУНТ"
                : isRecover
                  ? "ОТПРАВИТЬ ПИСЬМО"
                  : "ВОЙТИ"}
          </button>
        </form>
        {mode === "login" && (
          <button
            className="auth-text-button"
            type="button"
            onClick={() => switchMode("recover")}
          >
            Задать или сбросить пароль по email
          </button>
        )}
        {isRegister && confirmationEmail && (
          <button
            className="auth-text-button"
            type="button"
            disabled={busy}
            onClick={() => void resendConfirmation()}
          >
            Отправить письмо подтверждения ещё раз
          </button>
        )}
        {isRecover && (
          <button
            className="auth-text-button"
            type="button"
            onClick={() => switchMode("login")}
          >
            Вернуться ко входу
          </button>
        )}
        {error && (
          <p className="form-message form-message--error" role="alert">
            {error}
          </p>
        )}
        {notice && (
          <p className="form-message form-message--success" role="status">
            {notice}
          </p>
        )}
        <span className="auth-terms">
          Пароль используется только для MinLauncher. Для Minecraft сайт не
          просит пароль Microsoft: привязка откроет официальный вход Microsoft.
          Если Microsoft-аккаунта нет, создайте его на сайте Microsoft и
          вернитесь сюда.
        </span>
      </div>
    </AuthGateway>
  );
}

export function PasswordRecoveryPage({ session }: { session: Session | null }) {
  const [password, setPassword] = useState("");
  const [repeatPassword, setRepeatPassword] = useState("");
  const [email, setEmail] = useState("");
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    window.sessionStorage.removeItem("minlauncher:password-recovery");
  }, []);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError("");
    setNotice("");
    if (!supabase) return setError("Восстановление временно недоступно.");
    setBusy(true);
    try {
      if (!session) {
        if (!email.trim()) throw new Error("Введите адрес электронной почты.");
        const { error: resetError } = await supabase.auth.resetPasswordForEmail(
          email.trim().toLowerCase(),
          { redirectTo: `${window.location.origin}/auth/callback` },
        );
        if (resetError)
          throw new Error("Не удалось отправить письмо. Попробуйте позже.");
        setNotice(
          "Если адрес привязан к аккаунту, письмо для задания или сброса пароля отправлено.",
        );
      } else {
        const validation = validateNewPassword(password, repeatPassword);
        if (validation) throw new Error(validation);
        const { error: updateError } = await supabase.auth.updateUser({
          password,
        });
        if (updateError)
          throw new Error(
            "Не удалось сохранить пароль. Откройте ссылку из письма ещё раз.",
          );
        window.location.assign("/account/settings");
      }
    } catch (cause) {
      setError(
        cause instanceof Error ? cause.message : "Не удалось выполнить запрос.",
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <AuthWash variant="cave">
      <div className="auth-card">
        <a className="back-link" href="/account">
          ← Ко входу
        </a>
        <span className="eyebrow">ВОССТАНОВЛЕНИЕ ДОСТУПА</span>
        <h1>
          {session ? (
            <>
              НОВЫЙ
              <br />
              <em>ПАРОЛЬ.</em>
            </>
          ) : (
            <>
              ПИСЬМО
              <br />
              <em>ДЛЯ ВХОДА.</em>
            </>
          )}
        </h1>
        <form className="auth-form" onSubmit={(event) => void submit(event)}>
          {session ? (
            <>
              <label className="field">
                <span>НОВЫЙ ПАРОЛЬ</span>
                <input
                  type="password"
                  value={password}
                  onChange={(event) => setPassword(event.target.value)}
                  autoComplete="new-password"
                  minLength={12}
                  maxLength={72}
                  required
                />
              </label>
              <label className="field">
                <span>ПОВТОРИТЕ ПАРОЛЬ</span>
                <input
                  type="password"
                  value={repeatPassword}
                  onChange={(event) => setRepeatPassword(event.target.value)}
                  autoComplete="new-password"
                  minLength={12}
                  maxLength={72}
                  required
                />
              </label>
            </>
          ) : (
            <label className="field">
              <span>ЭЛЕКТРОННАЯ ПОЧТА</span>
              <input
                type="email"
                value={email}
                onChange={(event) => setEmail(event.target.value)}
                autoComplete="email"
                required
              />
            </label>
          )}
          <button
            className="button button--primary"
            type="submit"
            disabled={busy}
          >
            {busy
              ? "ПОДОЖДИТЕ…"
              : session
                ? "СОХРАНИТЬ ПАРОЛЬ"
                : "ОТПРАВИТЬ ПИСЬМО"}
          </button>
        </form>
        {error && (
          <p className="form-message form-message--error" role="alert">
            {error}
          </p>
        )}
        {notice && (
          <p className="form-message form-message--success" role="status">
            {notice}
          </p>
        )}
      </div>
    </AuthWash>
  );
}
