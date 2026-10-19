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
import { useEffect } from "react";

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
  /** The icon of the entry, drawn as text so that no font is needed. */
  glyph: string;
}

/** The entries of the sidebar. */
const NAV_ITEMS: NavItem[] = [
  { to: "/", label: "Играть", glyph: "▶" },
  { to: "/versions", label: "Версии", glyph: "≡" },
  { to: "/instances", label: "Инстансы", glyph: "◫" },
  { to: "/performance", label: "Производительность", glyph: "◷" },
  { to: "/settings", label: "Настройки", glyph: "⚙" },
];

/** The frame of the launcher. */
export function AppShell() {
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const running = useGameStore((state) => state.running);
  const installing = useInstallStore((state) => state.versionId);
  const installCompleted = useInstallStore((state) => state.completed);
  const installTotal = useInstallStore((state) => state.total);

  // The events of the game are listened to once, for the whole life of
  // the window. The listener is removed when the shell is unmounted.
  useEffect(() => {
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
      <aside className="shell__sidebar">
        <div className="shell__brand">
          <span className="shell__logo">ML</span>
          <span className="shell__name">MinLauncher</span>
        </div>

        <nav className="shell__nav">
          {NAV_ITEMS.map((item) => (
            <Link
              key={item.to}
              to={item.to}
              className="shell__link"
              activeProps={{ className: "shell__link--active" }}
              activeOptions={{ exact: item.to === "/" }}
            >
              <span className="shell__glyph" aria-hidden="true">
                {item.glyph}
              </span>
              {item.label}
            </Link>
          ))}
        </nav>

        <div className="shell__footer">
          {installing ? (
            <div className="shell__status shell__status--running">
              <span className="shell__dot" aria-hidden="true" />
              <div>
                <div className="shell__status-title">Установка</div>
                <div className="shell__status-sub">
                  {installing} — {installCompleted} из {installTotal}
                </div>
              </div>
            </div>
          ) : running ? (
            <div className="shell__status shell__status--running">
              <span className="shell__dot" aria-hidden="true" />
              <div>
                <div className="shell__status-title">Игра запущена</div>
                <div className="shell__status-sub">{running.version_id}</div>
              </div>
            </div>
          ) : (
            <div className="shell__status">
              <span className="shell__dot shell__dot--idle" aria-hidden="true" />
              <div className="shell__status-sub">Игра не запущена</div>
            </div>
          )}
        </div>
      </aside>

      <main className="shell__main" data-path={pathname}>
        <Outlet />
      </main>
    </div>
  );
}
