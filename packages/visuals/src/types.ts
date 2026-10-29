export type DioramaVariant = "island" | "cave" | "portal";
export type MotionState = "active" | "paused" | "reduced";

export interface SceneHandle {
  setRunning: (running: boolean) => void;
  dispose: () => void;
}

export interface SceneModule {
  mountScene: (
    host: HTMLElement,
    variant: DioramaVariant,
    fps: number,
    onFallback: () => void,
    mode: "site" | "launcher",
    onReady?: () => void,
  ) => SceneHandle;
}

export interface SceneSource {
  load: () => Promise<SceneModule>;
  posters: Record<DioramaVariant, string>;
}
