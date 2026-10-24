/**
 * The shell of the launcher: the sidebar, the header and the screen.
 *
 * The shell is the frame the screens live in. It knows about the
 * running game, because the state of the game is shown in the frame and
 * not inside a screen: the player has to see that the game is running
 * from every screen. The same is true of an installation, which takes
 * long enough that the player has to be able to leave the screen it
 * started on.
 */

import { Link, Outlet, useRouterState } from "@tanstack/react-router";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

import iconUrl from "../assets/icon.png";
import { LauncherUpdateNotice } from "./LauncherUpdateNotice";
import { watchGame } from "../stores/game";
import { useGameStore } from "../stores/game";
import { useInstallStore } from "../stores/install";
import { watchInstall } from "../stores/install";
import { watchRuntime } from "../stores/runtime";

/** One entry of the sidebar. */
interface NavItem {
  /** The route the entry leads to. */
  to: string;
  /** The name of the entry. */
  label: string;
}

/** The entries of the sidebar. */
const NAV_ITEMS: NavItem[] = [
  { to: "/", label: "Играть" },
  { to: "/versions", label: "Версии" },
  { to: "/instances", label: "Инстансы" },
  { to: "/downloads", label: "Загрузки" },
  { to: "/performance", label: "Производительность" },
  { to: "/settings", label: "Настройки" },
];

function NavIcon({ route }: { route: string }) {
  const icons: Record<string, ReactNode> = {
    "/": <path d="m5 3 7 5-7 5z" />,
    "/versions": (
      <>
        <path d="M4 3h9M4 8h9M4 13h9" />
        <path d="M1.5 3h.01M1.5 8h.01M1.5 13h.01" />
      </>
    ),
    "/instances": (
      <>
        <path d="m8 1 6 3.5v7L8 15l-6-3.5v-7z" />
        <path d="M2.2 4.7 8 8l5.8-3.3M8 8v7" />
      </>
    ),
    "/downloads": (
      <>
        <path d="M8 1v9m0 0 3.5-3.5M8 10 4.5 6.5" />
        <path d="M2 11v3h12v-3" />
      </>
    ),
    "/performance": (
      <>
        <path d="M2 13.5a6 6 0 1 1 12 0" />
        <path d="m8 10 3-3M4 13.5h8" />
      </>
    ),
    "/settings": (
      <>
        <path d="M8 2.5v1.2m0 8.6v1.2M13.5 8h-1.2m-8.6 0H2.5" />
        <circle cx="8" cy="8" r="3.2" />
        <circle cx="8" cy="8" r="6.1" />
      </>
    ),
  };
  return (
    <svg viewBox="0 0 16 16" aria-hidden="true">
      {icons[route]}
    </svg>
  );
}

const PAGE_TITLES: Record<string, string> = {
  "/": "MINECRAFT",
  "/versions": "БИБЛИОТЕКА",
  "/instances": "ПРОФИЛИ",
  "/downloads": "ФАЙЛЫ И УСТАНОВКА",
  "/performance": "ЗАМЕРЫ",
  "/settings": "ПАРАМЕТРЫ",
};

/** The frame of the launcher. */
export function AppShell() {
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const viewportRef = useRef<HTMLDivElement>(null);
  const running = useGameStore((state) => state.running);
  const restarting = useGameStore((state) => state.restarting);
  const installing = useInstallStore((state) => state.versionId);
  const installCompleted = useInstallStore((state) => state.completed);
  const installTotal = useInstallStore((state) => state.total);
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    viewportRef.current?.scrollTo({ top: 0, behavior: "auto" });
  }, [pathname]);

  useEffect(() => {
    if (!isTauri()) return;
    const window = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    void window
      .isMaximized()
      .then(setMaximized)
      .catch(() => undefined);
    void window
      .onResized(() => {
        void window
          .isMaximized()
          .then(setMaximized)
          .catch(() => undefined);
      })
      .then((stop) => {
        unlisten = stop;
      })
      .catch(() => undefined);
    return () => unlisten?.();
  }, []);

  // The events of the game are listened to once, for the whole life of
  // the window. The listener is removed when the shell is unmounted.
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    void watchGame().then((stop) => {
      if (cancelled) {
        stop();
        return;
      }
      unlisten = stop;
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // The events of an installation are listened to in the same way.
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    void watchInstall().then((stop) => {
      if (cancelled) {
        stop();
        return;
      }
      unlisten = stop;
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // The events of a download of a Java runtime are listened to in the
  // same way.
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    void watchRuntime().then((stop) => {
      if (cancelled) {
        stop();
        return;
      }
      unlisten = stop;
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  return (
    <div className="shell">
      <a
        className="skip-link"
        href="#main-content"
        onClick={(event) => {
          event.preventDefault();
          const main = document.getElementById("main-content");
          main?.focus();
          main?.querySelector(".shell__viewport")?.scrollTo({ top: 0 });
        }}
      >
        К содержимому
      </a>
      <header className="shell__windowbar">
        <div className="shell__windowbar-drag" data-tauri-drag-region>
          <img className="shell__window-logo" src={iconUrl} alt="" />
          <span className="shell__window-name">MINLAUNCHER</span>
          <span className="shell__window-divider" />
          <span className="shell__window-route">{PAGE_TITLES[pathname] ?? "ИГРОВАЯ СРЕДА"}</span>
        </div>
        <div className="shell__windowbar-right">
          <span className={running ? "shell__live shell__live--on" : "shell__live"}>
            <span className="shell__live-dot" aria-hidden="true" />
            {running ? "Игра запущена" : "Готов к работе"}
          </span>
          <span className="shell__topbar-version">V 0.1.0</span>
          <div className="window-controls">
            <button
              className="window-controls__button"
              aria-label="Свернуть"
              onClick={() => void getCurrentWindow().minimize()}
            >
              <svg viewBox="0 0 12 12" aria-hidden="true">
                <path d="M2 6h8" />
              </svg>
            </button>
            <button
              className="window-controls__button"
              aria-label={maximized ? "Восстановить" : "Развернуть"}
              onClick={() => void getCurrentWindow().toggleMaximize()}
            >
              <svg viewBox="0 0 12 12" aria-hidden="true">
                {maximized ? (
                  <>
                    <path d="M4 2.5h5.5V8" />
                    <rect x="2.5" y="4" width="5.5" height="5.5" rx=".5" />
                  </>
                ) : (
                  <rect x="2.5" y="2.5" width="7" height="7" rx=".5" />
                )}
              </svg>
            </button>
            <button
              className="window-controls__button window-controls__button--close"
              aria-label="Закрыть"
              onClick={() => void getCurrentWindow().close()}
            >
              <svg viewBox="0 0 12 12" aria-hidden="true">
                <path d="m3 3 6 6M9 3 3 9" />
              </svg>
            </button>
          </div>
        </div>
      </header>
      <aside className="shell__sidebar">
        <div className="shell__nav-label">Игра</div>
        <nav className="shell__nav" aria-label="Основная навигация">
          {NAV_ITEMS.slice(0, 1).map((item) => (
            <Link
              key={item.to}
              to={item.to}
              className="shell__link"
              title={item.label}
              aria-label={item.label}
              activeProps={{ className: "shell__link--active" }}
              activeOptions={{ exact: item.to === "/" }}
            >
              <span className="shell__glyph">
                <NavIcon route={item.to} />
              </span>
              {item.label}
            </Link>
          ))}
        </nav>

        <div className="shell__nav-label shell__nav-label--manage">Управление</div>
        <nav className="shell__nav shell__nav--secondary" aria-label="Управление">
          {NAV_ITEMS.slice(1).map((item) => (
            <Link
              key={item.to}
              to={item.to}
              className="shell__link"
              title={item.label}
              aria-label={item.label}
              activeProps={{ className: "shell__link--active" }}
            >
              <span className="shell__glyph">
                <NavIcon route={item.to} />
              </span>
              {item.label}
            </Link>
          ))}
        </nav>

        <div className="shell__footer">
          {installing ? (
            <div className="shell__status shell__status--running" role="status" aria-live="polite">
              <span className="shell__dot" aria-hidden="true" />
              <div>
                <div className="shell__status-title">Установка</div>
                <div className="shell__status-sub">
                  {installing} — {installCompleted} из {installTotal}
                </div>
              </div>
            </div>
          ) : restarting ? (
            <div className="shell__status shell__status--running" role="status" aria-live="polite">
              <span className="shell__dot" aria-hidden="true" />
              <div>
                <div className="shell__status-title">Игра перезапускается</div>
                <div className="shell__status-sub">Попытка {restarting}</div>
              </div>
            </div>
          ) : running ? (
            <div className="shell__status shell__status--running" role="status" aria-live="polite">
              <span className="shell__dot" aria-hidden="true" />
              <div>
                <div className="shell__status-title">Игра запущена</div>
                <div className="shell__status-sub">{running.version_id}</div>
              </div>
            </div>
          ) : (
            <div className="shell__status" role="status" aria-live="polite">
              <span className="shell__dot shell__dot--idle" aria-hidden="true" />
              <div className="shell__status-sub">Игра не запущена</div>
            </div>
          )}
        </div>
      </aside>

      <main id="main-content" className="shell__main" data-path={pathname} tabIndex={-1}>
        <div ref={viewportRef} className="shell__viewport">
          {pathname === "/settings" ? null : <LauncherUpdateNotice />}
          <Outlet />
        </div>
      </main>
    </div>
  );
}
