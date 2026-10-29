import { useEffect, useRef, useState } from "react";
import { registerScene } from "./ownership";
import type {
  DioramaVariant,
  MotionState,
  SceneHandle,
  SceneSource,
} from "./types";

export interface DioramaProps {
  variant: DioramaVariant;
  motion?: MotionState;
  mode?: "site" | "launcher";
  className?: string;
}

interface DioramaCoreProps extends DioramaProps {
  scene: SceneSource;
}

export function DioramaCore({
  variant,
  motion = "active",
  mode = "site",
  className = "",
  scene,
}: DioramaCoreProps) {
  const root = useRef<HTMLDivElement>(null);
  const host = useRef<HTMLDivElement>(null);
  const [compact, setCompact] = useState(() => innerWidth <= 720);
  const [reduced, setReduced] = useState(
    () =>
      typeof matchMedia === "function" &&
      matchMedia("(prefers-reduced-motion: reduce)").matches,
  );
  const [visible, setVisible] = useState(!document.hidden);
  const [focused, setFocused] = useState(
    () => mode !== "launcher" || document.hasFocus(),
  );
  const [ratio, setRatio] = useState(0);
  const [owns, setOwns] = useState(false);
  const [rendered, setRendered] = useState(false);
  const [failed, setFailed] = useState(false);
  const enabled =
    !reduced &&
    motion === "active" &&
    visible &&
    (mode !== "launcher" || focused) &&
    !failed;

  useEffect(() => {
    const element = root.current;
    if (!element) return;
    const visibility = () => setVisible(!document.hidden);
    const focus = () => setFocused(true);
    const blur = () => setFocused(false);
    const nativeFocus = (event: Event) =>
      setFocused((event as CustomEvent<boolean>).detail);
    const media =
      typeof matchMedia === "function"
        ? matchMedia("(prefers-reduced-motion: reduce)")
        : null;
    const sizeMedia =
      typeof matchMedia === "function"
        ? matchMedia("(max-width: 720px)")
        : null;
    const sizePreference = () =>
      setCompact(sizeMedia?.matches ?? innerWidth <= 720);
    sizeMedia?.addEventListener("change", sizePreference);
    const preference = () => setReduced(media?.matches ?? false);
    media?.addEventListener("change", preference);
    document.addEventListener("visibilitychange", visibility);
    window.addEventListener("focus", focus);
    window.addEventListener("blur", blur);
    if (mode === "launcher") {
      window.addEventListener("minlauncher:window-focus", nativeFocus);
    }
    const observer =
      typeof IntersectionObserver !== "undefined"
        ? new IntersectionObserver(
            (entries) => {
              const entry = entries[0];
              setRatio(entry?.isIntersecting ? entry.intersectionRatio : 0);
            },
            { threshold: [0, 0.1, 0.25, 0.5, 0.75, 1] },
          )
        : null;
    observer?.observe(element);
    return () => {
      observer?.disconnect();
      media?.removeEventListener("change", preference);
      sizeMedia?.removeEventListener("change", sizePreference);
      document.removeEventListener("visibilitychange", visibility);
      window.removeEventListener("focus", focus);
      window.removeEventListener("blur", blur);
      window.removeEventListener("minlauncher:window-focus", nativeFocus);
    };
  }, [mode]);

  const ownership = useRef<ReturnType<typeof registerScene> | null>(null);
  useEffect(() => {
    const registration = registerScene(setOwns);
    ownership.current = registration;
    return () => {
      registration.dispose();
      ownership.current = null;
    };
  }, []);
  useEffect(() => {
    ownership.current?.update(enabled ? ratio : 0);
  }, [enabled, ratio]);

  useEffect(() => {
    const element = host.current;
    if (
      !owns ||
      !enabled ||
      !element ||
      typeof WebGLRenderingContext === "undefined"
    )
      return;
    let cancelled = false,
      handle: SceneHandle | undefined;
    // Give the interface its first paint before fetching the renderer.
    const timer = window.setTimeout(() => {
      void scene
        .load()
        .then(({ mountScene }) => {
          if (cancelled) return;
          handle = mountScene(
            element,
            variant,
            mode === "launcher" || compact ? 30 : 60,
            () => {
              if (!cancelled) {
                setFailed(true);
                setRendered(false);
              }
            },
            mode,
            () => {
              if (!cancelled) setRendered(true);
            },
          );
          handle.setRunning(true);
          if (mode === "launcher") setRendered(true);
        })
        .catch((error: unknown) => {
          if (!cancelled) {
            console.warn("MinLauncher diorama unavailable", error);
            setFailed(true);
          }
        });
    }, 160);
    return () => {
      cancelled = true;
      clearTimeout(timer);
      handle?.dispose();
      setRendered(false);
    };
  }, [owns, enabled, variant, mode, compact, scene]);

  return (
    <div
      ref={root}
      className={`ml-diorama ml-diorama--${variant} ${className}`}
      data-rendered={rendered}
      data-motion={rendered ? "active" : "paused"}
    >
      <img
        className="ml-diorama__poster"
        src={scene.posters[variant]}
        alt=""
        aria-hidden="true"
        decoding="async"
      />
      <div ref={host} className="ml-diorama__stage" aria-hidden="true" />
    </div>
  );
}
