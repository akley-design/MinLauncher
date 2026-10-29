import { DioramaCore, type DioramaProps } from "./DioramaCore";
import { scenePosters } from "./posters";
import type { SceneSource } from "./types";

const legacyScene: SceneSource = {
  load: () => import("./runtime"),
  posters: scenePosters,
};

export type { DioramaProps };

export function Diorama(props: DioramaProps) {
  return <DioramaCore {...props} scene={legacyScene} />;
}
