import { useEffect, useState } from "react";
import type { Session } from "@supabase/supabase-js";

export function useAccountSession(immediate: boolean) {
  const [session, setSession] = useState<Session | null>(null);
  const [authLoading, setAuthLoading] = useState(false);

  useEffect(() => {
    let active = true;
    let started = false;
    let subscription: { unsubscribe: () => void } | undefined;
    let idle: number | undefined;
    let timer: number | undefined;
    const idleWindow = window as Window & {
      requestIdleCallback?: (
        callback: () => void,
        options?: { timeout: number },
      ) => number;
      cancelIdleCallback?: (id: number) => void;
    };

    const start = () => {
      if (started || !active) return;
      started = true;
      setAuthLoading(true);
      void import("./supabase")
        .then(({ supabase }) => {
          if (!active || !supabase) {
            if (active) setAuthLoading(false);
            return;
          }
          const { data } = supabase.auth.onAuthStateChange(
            (event, nextSession) => {
              if (event === "PASSWORD_RECOVERY") {
                window.sessionStorage.setItem(
                  "minlauncher:password-recovery",
                  "1",
                );
                window.setTimeout(
                  () => window.location.replace("/auth/reset-password"),
                  0,
                );
              }
              if (active) {
                setSession(nextSession);
                setAuthLoading(false);
              }
            },
          );
          subscription = data.subscription;
          void supabase.auth
            .getSession()
            .then(({ data: current, error }) => {
              if (!active) return;
              setSession(current.session);
              setAuthLoading(false);
              if (error)
                console.error("Could not restore Supabase session", error);
            })
            .catch((error: unknown) => {
              if (!active) return;
              console.error("Could not restore Supabase session", error);
              setAuthLoading(false);
            });
        })
        .catch((error: unknown) => {
          if (!active) return;
          console.error("Could not load Supabase client", error);
          setAuthLoading(false);
        });
    };

    if (immediate) {
      start();
    } else if (idleWindow.requestIdleCallback) {
      idle = idleWindow.requestIdleCallback(start, { timeout: 2500 });
    } else {
      timer = window.setTimeout(start, 2000);
    }

    return () => {
      active = false;
      subscription?.unsubscribe();
      if (idle !== undefined) idleWindow.cancelIdleCallback?.(idle);
      if (timer !== undefined) clearTimeout(timer);
    };
  }, [immediate]);

  return { session, authLoading };
}
