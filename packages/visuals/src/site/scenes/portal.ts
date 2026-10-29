import { Mesh, PlaneGeometry } from "three";
import {
  addCrystalCluster,
  addGrassTuft,
  center,
  SceneBuilder,
  S,
} from "../kit";
import { PAL } from "../palette";
import { hash3 } from "../motion";
import { createPortalMaterial } from "../shaders";

export function buildPortal(ctx: SceneBuilder) {
  const inside = (x: number, z: number, shrink = 0) => {
    const nx = (x + 0.5) / (6.7 - shrink);
    const nz = (z + 0.5) / (5.8 - shrink);
    return nx * nx + nz * nz <= 1;
  };

  for (let x = -7; x <= 6; x++) {
    for (let z = -6; z <= 5; z++) {
      if (!inside(x, z)) continue;
      for (let level = 0; level <= 5; level++) {
        if (level > 0 && !inside(x, z, 0.7 + level * 0.85)) break;
        const pick = hash3(x, z, level + 23);
        const style =
          level === 0
            ? Math.abs(x) <= 3 && Math.abs(z) <= 1
              ? pick > 0.4
                ? PAL.obsidian
                : PAL.obsidianDark
              : pick > 0.78
                ? PAL.grassDark
                : pick > 0.45
                  ? PAL.deepslate
                  : PAL.deepslateDark
            : level <= 2
              ? pick > 0.5
                ? PAL.deepslate
                : PAL.deepslateDark
              : pick > 0.5
                ? PAL.stone
                : PAL.stoneDark;
        ctx.block(x, -level, z, style);
      }
    }
  }

  for (let x = -3; x <= 2; x++) {
    for (let y = 1; y <= 7; y++) {
      if (x === -3 || x === 2 || y === 7) {
        ctx.block(
          x,
          y,
          -1,
          (x + y) % 3 === 0 ? PAL.obsidianDark : PAL.obsidian,
        );
      }
    }
  }
  const portalGeometry = new PlaneGeometry(4 * S, 6 * S, 8, 14);
  const portalMaterial = createPortalMaterial();
  ctx.resources.add(portalGeometry);
  ctx.resources.add(portalMaterial);
  ctx.timeUniforms.push(portalMaterial.uniforms.uTime!);
  const portal = new Mesh(portalGeometry, portalMaterial);
  portal.name = "Portal_Energy_Surface";
  portal.position.set(0, 4 * S, 0.02);
  ctx.group.add(portal);
  ctx.movers.push((time) => {
    portalMaterial.uniforms.uPulse!.value = 0.93 + 0.07 * Math.sin(time * 0.8);
  });

  ctx.fill(-5, 1, 1, 1, 4, 1, PAL.deepslateDark);
  ctx.detail(-5.15, 4.9, 0.85, 1.3, 0.25, 1.3, PAL.obsidian);
  ctx.fill(4, 1, -3, 1, 2, 1, PAL.obsidianDark);
  ctx.detail(3.85, 2.9, -3.15, 1.3, 0.2, 1.3, PAL.obsidian);

  ctx.detail(-2.5, 1, 2.3, 5, 0.12, 0.9, PAL.deepslate);
  ctx.detail(-3.2, 1, 3.2, 6.4, 0.12, 0.9, PAL.deepslateDark);
  ctx.detail(-2, 1, 4, 4, 0.12, 0.8, PAL.obsidianDark);

  for (const [x, z] of [
    [-5, 3],
    [4, 2],
    [-4, -3],
    [3, -4],
    [0, 4],
  ] as const) {
    addGrassTuft(ctx, x, z, 1, 0.55 + hash3(x, z, 5) * 0.25);
  }
  for (const [x, z] of [
    [-4, 2],
    [3, 1],
  ] as const) {
    addCrystalCluster(ctx, center(x, 1, z)[0], 0.34, center(x, 1, z)[2], 0.75);
  }

  const pool = ctx.pool("#9a4cff", [0, 0.36, 0.45], 2.2, 0.32);
  const glow = ctx.glow("#b25dff", [0, 1.25, 0.35], 2.2, 0.28);
  const mist = ctx.glow("#8f43e8", [0, 1.9, -0.15], 3, 0.05);
  const light = ctx.light("#b45cff", 3.4, 6, [0, 1.4, 0.55]);
  ctx.movers.push((time) => {
    const pulse = 0.5 + 0.5 * Math.sin(time * 0.9);
    const surge = Math.pow(Math.max(0, Math.sin(time * 0.31)), 8);
    light.intensity = 2.9 + pulse * 0.9 + surge * 1.8;
    glow.material.opacity = 0.2 + pulse * 0.14 + surge * 0.12;
    pool.material.opacity = 0.25 + pulse * 0.1 + surge * 0.08;
    mist.material.opacity = 0.035 + pulse * 0.03;
  });

  ctx.particles({
    count: 34,
    motion: "ember",
    colors: ["#e2b8ff", "#b56bff", "#f3ddff"],
    origin: [0, 1.15, 0.4],
    spread: [1.1, 0.5, 0.4],
    size: 4.5,
    sizeJitter: 0.5,
    speed: 0.18,
    opacity: 0.85,
    additive: true,
  });
  ctx.particles({
    count: 18,
    motion: "dust",
    colors: ["#b98cff", "#8d5ac2", "#e5c8ff"],
    origin: [0, 1.6, 0.2],
    spread: [2.4, 2, 1.2],
    size: 4,
    sizeJitter: 0.6,
    speed: 0.04,
    opacity: 0.22,
    additive: true,
  });
}
