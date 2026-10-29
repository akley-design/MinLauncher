import { DioramaCore, type DioramaProps } from "./DioramaCore";
import { scenePostersSite } from "./site/posters";
import type { SceneSource } from "./types";

const siteScene: SceneSource = {
  load: () => import("./site/runtime-site"),
  posters: scenePostersSite,
};

export type SiteDioramaProps = Omit<DioramaProps, "mode" | "reuse">;

export function SiteDiorama({
  variant,
  motion,
  className,
  poster,
  priority,
  lazy,
}: SiteDioramaProps) {
  return (
    <DioramaCore
      variant={variant}
      motion={motion}
      mode="site"
      className={className}
      poster={poster}
      priority={priority}
      lazy={lazy}
      reuse
      scene={siteScene}
    />
  );
}

export { scenePostersSite };
