export type DioramaVariant = "island" | "cave" | "portal";
export type MotionState = "active" | "paused" | "reduced";

export interface SceneQuality {
  fps: number;
  compact: boolean;
}

export interface SceneMountOptions {
  compact?: boolean;
  adaptive?: boolean;
  attach?: boolean;
}

export interface SceneHandle {
  setRunning: (running: boolean) => void;
  setActive?: (active: boolean) => void;
  setQuality?: (quality: SceneQuality) => void;
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
    options?: SceneMountOptions,
  ) => SceneHandle;
}

export interface SceneSource {
  load: () => Promise<SceneModule>;
  posters: Record<DioramaVariant, string>;
}

export interface PosterSources {
  avif: string;
  avifSet: string;
  webp: string;
  webpSet: string;
  width: number;
  height: number;
  sizes?: string;
}
