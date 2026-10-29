import { useEffect, useRef, useState } from "react";
import { registerScene } from "./ownership";
import type {
  DioramaVariant,
  MotionState,
  PosterSources,
  SceneHandle,
  SceneSource,
} from "./types";

export interface DioramaProps {
  variant: DioramaVariant;
  motion?: MotionState;
  mode?: "site" | "launcher";
  className?: string;
  reuse?: boolean;
  poster?: PosterSources;
  priority?: boolean;
  lazy?: boolean;
}

interface DioramaCoreProps extends DioramaProps {
  scene: SceneSource;
}

function isBrowser() {
  return typeof window !== "undefined" && typeof document !== "undefined";
}

export function DioramaCore({
  variant,
  motion = "active",
  mode = "site",
  className = "",
  reuse = false,
  poster,
  priority = false,
  lazy = false,
  scene,
}: DioramaCoreProps) {
  const root = useRef<HTMLDivElement>(null);
  const host = useRef<HTMLDivElement>(null);
  const handle = useRef<SceneHandle | null>(null);
  const [compact, setCompact] = useState(() =>
    isBrowser() ? window.innerWidth <= 720 : false,
  );
  const [reduced, setReduced] = useState(
    () =>
      isBrowser() &&
      typeof matchMedia === "function" &&
      matchMedia("(prefers-reduced-motion: reduce)").matches,
  );
  const [visible, setVisible] = useState(() =>
    isBrowser() ? !document.hidden : true,
  );
  const [focused, setFocused] = useState(
    () => !isBrowser() || mode !== "launcher" || document.hasFocus(),
  );
  const [ratio, setRatio] = useState(0);
  const [near, setNear] = useState(false);
  const [owns, setOwns] = useState(false);
  const [active, setActive] = useState(false);
  const [rendered, setRendered] = useState(false);
  const [failed, setFailed] = useState(false);
  const enabled =
    !reduced &&
    motion === "active" &&
    visible &&
    (mode !== "launcher" || focused) &&
    !failed;
  const activeWanted = reuse && owns && enabled;

  const activeRef = useRef(activeWanted);
  activeRef.current = activeWanted;
  const compactRef = useRef(compact);
  compactRef.current = compact;

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
      setCompact(sizeMedia?.matches ?? window.innerWidth <= 720);
    sizeMedia?.addEventListener("change", sizePreference);
    const preference = () => setReduced(media?.matches ?? false);
    media?.addEventListener("change", preference);
    document.addEventListener("visibilitychange", visibility);
    window.addEventListener("focus", focus);
    window.addEventListener("blur", blur);
    if (mode === "launcher") {
      window.addEventListener("minlauncher:window-focus", nativeFocus);
    }
    const ownershipObserver =
      typeof IntersectionObserver !== "undefined"
        ? new IntersectionObserver(
            (entries) => {
              const entry = entries[0];
              setRatio(entry?.isIntersecting ? entry.intersectionRatio : 0);
            },
            { threshold: [0, 0.1, 0.25, 0.5, 0.75, 1] },
          )
        : null;
    ownershipObserver?.observe(element);
    let nearObserver: IntersectionObserver | null = null;
    if (reuse) {
      if (typeof IntersectionObserver !== "undefined") {
        nearObserver = new IntersectionObserver(
          (entries) => setNear(Boolean(entries[0]?.isIntersecting)),
          { rootMargin: "400px 0px", threshold: 0 },
        );
        nearObserver.observe(element);
      } else {
        setNear(true);
      }
    }
    return () => {
      ownershipObserver?.disconnect();
      nearObserver?.disconnect();
      media?.removeEventListener("change", preference);
      sizeMedia?.removeEventListener("change", sizePreference);
      document.removeEventListener("visibilitychange", visibility);
      window.removeEventListener("focus", focus);
      window.removeEventListener("blur", blur);
      window.removeEventListener("minlauncher:window-focus", nativeFocus);
    };
  }, [mode, reuse]);

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
    if (reuse) return;
    const element = host.current;
    if (
      !owns ||
      !enabled ||
      !element ||
      typeof WebGLRenderingContext === "undefined"
    )
      return;
    let cancelled = false;
    let instance: SceneHandle | undefined;
    // Give the interface its first paint before fetching the renderer.
    const timer = window.setTimeout(() => {
      void scene
        .load()
        .then(({ mountScene }) => {
          if (cancelled) return;
          instance = mountScene(
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
          instance.setRunning(true);
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
      instance?.dispose();
      setRendered(false);
    };
  }, [reuse, owns, enabled, variant, mode, compact, scene]);

  useEffect(() => {
    if (!reuse) return;
    const element = host.current;
    if (
      !near ||
      failed ||
      reduced ||
      !visible ||
      !element ||
      typeof WebGLRenderingContext === "undefined"
    )
      return;
    if (handle.current) return;
    let cancelled = false;
    let idle: number | undefined;
    let timer: number | undefined;

    const mount = () => {
      if (cancelled || handle.current) return;
      void scene
        .load()
        .then(({ mountScene }) => {
          if (cancelled || handle.current) return;
          const instance = mountScene(
            element,
            variant,
            compactRef.current ? 30 : 60,
            () => {
              if (!cancelled) {
                setFailed(true);
                setRendered(false);
              }
            },
            "site",
            () => {
              if (!cancelled) setRendered(true);
            },
            {
              compact: compactRef.current,
              adaptive: true,
              attach: false,
            },
          );
          handle.current = instance;
          instance.setQuality?.({
            fps: compactRef.current ? 30 : 60,
            compact: compactRef.current,
          });
          instance.setActive?.(activeRef.current);
          instance.setRunning(activeRef.current);
        })
        .catch((error: unknown) => {
          if (!cancelled) {
            console.warn("MinLauncher diorama unavailable", error);
            setFailed(true);
          }
        });
    };

    const idleWindow = window as Window & {
      requestIdleCallback?: (
        callback: () => void,
        options?: { timeout: number },
      ) => number;
      cancelIdleCallback?: (id: number) => void;
    };
    if (idleWindow.requestIdleCallback) {
      idle = idleWindow.requestIdleCallback(mount, { timeout: 1500 });
    } else {
      timer = window.setTimeout(mount, 200);
    }

    return () => {
      cancelled = true;
      if (idle !== undefined) idleWindow.cancelIdleCallback?.(idle);
      if (timer !== undefined) clearTimeout(timer);
    };
  }, [reuse, near, failed, reduced, visible, variant, scene]);

  useEffect(() => {
    if (!reuse) return;
    const instance = handle.current;
    instance?.setActive?.(activeWanted);
    instance?.setRunning(activeWanted);
    setActive(activeWanted);
  }, [reuse, activeWanted]);

  useEffect(() => {
    if (!reuse) return;
    handle.current?.setQuality?.({
      fps: compact ? 30 : 60,
      compact,
    });
  }, [reuse, compact]);

  useEffect(() => {
    if (!failed) return;
    handle.current?.dispose();
    handle.current = null;
    setRendered(false);
    setActive(false);
  }, [failed]);

  useEffect(
    () => () => {
      handle.current?.dispose();
      handle.current = null;
    },
    [],
  );

  return (
    <div
      ref={root}
      className={`ml-diorama ml-diorama--${variant} ${className}`}
      data-rendered={rendered}
      data-active={reuse ? active : undefined}
      data-motion={rendered ? "active" : "paused"}
    >
      {poster ? (
        <picture className="ml-diorama__poster">
          <source
            type="image/avif"
            srcSet={poster.avifSet}
            sizes={poster.sizes ?? "(max-width: 720px) 92vw, 620px"}
          />
          <source
            type="image/webp"
            srcSet={poster.webpSet}
            sizes={poster.sizes ?? "(max-width: 720px) 92vw, 620px"}
          />
          <img
            src={poster.webp}
            srcSet={poster.webpSet}
            sizes={poster.sizes ?? "(max-width: 720px) 92vw, 620px"}
            width={poster.width}
            height={poster.height}
            alt=""
            aria-hidden="true"
            loading={priority ? "eager" : lazy ? "lazy" : "eager"}
            fetchPriority={priority ? "high" : undefined}
            decoding="async"
          />
        </picture>
      ) : (
        <img
          className="ml-diorama__poster"
          src={scene.posters[variant]}
          alt=""
          aria-hidden="true"
          decoding="async"
          loading={lazy ? "lazy" : "eager"}
        />
      )}
      <div ref={host} className="ml-diorama__stage" aria-hidden="true" />
    </div>
  );
}
