import { DioramaCore, type DioramaProps } from "./DioramaCore";
import {
  sceneCoversSite,
  scenePostersSite,
  sceneWidesSite,
  sitePosterThumb,
} from "./site/posters";
import type { SceneSource } from "./types";

const siteScene: SceneSource = {
  load: () => import("./site/runtime-site"),
  posters: scenePostersSite,
};

export type SiteDioramaProps = Omit<DioramaProps, "mode">;

export function SiteDiorama({ variant, motion, className }: SiteDioramaProps) {
  return (
    <DioramaCore
      variant={variant}
      motion={motion}
      mode="site"
      className={className}
      scene={siteScene}
    />
  );
}

export { sceneCoversSite, scenePostersSite, sceneWidesSite, sitePosterThumb };
