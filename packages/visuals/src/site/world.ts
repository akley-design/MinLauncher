import type { Group } from "three";
import { SceneBuilder } from "./kit";
import { buildIsland } from "./scenes/island";
import { buildCave } from "./scenes/cave";
import { buildPortal } from "./scenes/portal";
import { easeOutCubic } from "./motion";
import type { DioramaVariant } from "../types";

export interface SiteWorld {
  group: Group;
  triangles: number;
  animate: (time: number, reduced: boolean) => void;
  setPixelRatio: (ratio: number) => void;
  dispose: () => void;
}

export function buildSiteWorld(
  variant: DioramaVariant,
  anisotropy = 1,
): SiteWorld {
  const ctx = new SceneBuilder(anisotropy);
  if (variant === "island") buildIsland(ctx);
  else if (variant === "cave") buildCave(ctx);
  else buildPortal(ctx);
  const stats = ctx.finish();

  return {
    group: ctx.group,
    triangles: stats.triangles,
    animate(time, reduced) {
      if (reduced) {
        for (const object of ctx.hidden) object.visible = false;
        for (const uniform of ctx.timeUniforms) uniform.value = 0;
        ctx.group.position.set(0, 0, 0);
        ctx.group.rotation.set(0, 0, 0);
        ctx.group.scale.setScalar(1);
        return;
      }
      const settle = easeOutCubic(time / 1.15);
      ctx.group.position.set(
        0,
        -(1 - settle) * 0.12 + Math.sin(time * 0.55) * 0.028,
        0,
      );
      ctx.group.rotation.y = Math.sin(time * 0.18) * 0.018;
      ctx.group.scale.setScalar(0.986 + 0.014 * settle);
      for (const uniform of ctx.timeUniforms) uniform.value = time;
      for (const mover of ctx.movers) mover(time);
    },
    setPixelRatio(ratio) {
      ctx.setPixelRatio(ratio);
    },
    dispose() {
      ctx.resources.dispose();
    },
  };
}
