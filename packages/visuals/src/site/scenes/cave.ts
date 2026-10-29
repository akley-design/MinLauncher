import { Mesh, MeshBasicMaterial, PlaneGeometry, SphereGeometry } from "three";
import {
  addCrystalCluster,
  addGrassTuft,
  addMineCart,
  addPine,
  addRails,
  addFlower,
  center,
  corner,
  SceneBuilder,
} from "../kit";
import { PAL } from "../palette";
import { fract, hash3 } from "../motion";

function addLantern(ctx: SceneBuilder, x: number, z: number, phase: number) {
  ctx.detail(x + 0.35, 1, z + 0.35, 0.3, 2, 0.3, PAL.planksDark);
  ctx.detail(x + 0.18, 2.96, z + 0.18, 0.64, 0.42, 0.64, PAL.lantern);
  ctx.detail(x + 0.08, 3.38, z + 0.08, 0.84, 0.12, 0.84, PAL.planksDark);
  const point = corner(x + 0.5, 3.16, z + 0.5);
  const glow = ctx.glow("#ffcc8c", point, 1.25, 0.55);
  const light = ctx.light("#ffbe78", 2.1, 4.2, point);
  ctx.movers.push((time) => {
    const flicker =
      0.72 +
      0.16 * Math.sin(time * 6.7 + phase) +
      0.08 * Math.sin(time * 13.1 + phase * 2.3);
    light.intensity = 1.2 + flicker * 1.2;
    glow.material.opacity = 0.22 + flicker * 0.38;
  });
}

export function buildCave(ctx: SceneBuilder) {
  const insideBase = (x: number, z: number, shrink = 0) => {
    const nx = (x + 0.5) / (8.2 - shrink);
    const nz = (z + 0.5) / (6.4 - shrink);
    return nx * nx + nz * nz <= 1;
  };

  for (let x = -8; x <= 7; x++) {
    for (let z = -6; z <= 5; z++) {
      if (!insideBase(x, z)) continue;
      for (let level = 0; level <= 5; level++) {
        if (level > 0 && !insideBase(x, z, 0.7 + level * 0.85)) break;
        const pick = hash3(x, z, level + 11);
        const style =
          level === 0
            ? pick > 0.82
              ? PAL.grassDark
              : pick > 0.52
                ? PAL.stone
                : PAL.stoneDark
            : level <= 2
              ? pick > 0.5
                ? PAL.stone
                : PAL.stoneDark
              : pick > 0.5
                ? PAL.deepslate
                : PAL.deepslateDark;
        ctx.block(x, -level, z, style);
      }
    }
  }

  const inHill = (x: number, z: number) => {
    const nx = (x + 0.5) / 5.6;
    const nz = (z + 3.5) / 3.7;
    return nx * nx + nz * nz <= 1;
  };
  for (let x = -6; x <= 5; x++) {
    for (let z = -7; z <= -1; z++) {
      if (!inHill(x, z)) continue;
      for (let y = 1; y <= 4; y++) {
        if (x >= -1 && x <= 1 && y <= 3 && z >= -5) continue;
        const pick = hash3(x, y, z);
        const style =
          y === 4
            ? pick > 0.74
              ? PAL.grassDark
              : pick > 0.4
                ? PAL.stone
                : PAL.stoneDark
            : y === 1
              ? pick > 0.5
                ? PAL.stoneDark
                : PAL.deepslate
              : pick > 0.5
                ? PAL.stone
                : PAL.stoneDark;
        ctx.block(x, y, z, style);
      }
    }
  }

  for (let x = -1; x <= 1; x++)
    for (let y = 1; y <= 3; y++) ctx.block(x, y, -6, PAL.obsidianDark);
  const voidGeometry = new PlaneGeometry(1.5, 1.5);
  const voidMaterial = new MeshBasicMaterial({
    color: 0x040706,
    toneMapped: false,
  });
  ctx.resources.add(voidGeometry);
  ctx.resources.add(voidMaterial);
  const tunnelVoid = new Mesh(voidGeometry, voidMaterial);
  tunnelVoid.position.set(0.17, 0.72, -1.69);
  ctx.group.add(tunnelVoid);
  const tunnelMist = ctx.glow("#4a9e8f", [0.17, 0.68, -1.62], 0.55, 0.15);
  ctx.block(-2, 3, -5, PAL.deepslateDark);
  ctx.block(2, 3, -4, PAL.deepslateDark);
  ctx.block(-2, 2, -3, PAL.deepslate);
  ctx.block(2, 2, -3, PAL.deepslate);

  ctx.block(-4, 1, -1, PAL.oreCopper);
  ctx.block(-3, 2, -1, PAL.oreAmethyst);
  ctx.block(-3, 1, -1, PAL.oreCopper);
  ctx.block(3, 2, -1, PAL.oreCopper);
  ctx.block(4, 1, -1, PAL.oreCopper);
  ctx.block(3, 3, -1, PAL.oreCopper);

  for (const [x, z] of [
    [-4, -3],
    [-2, -5],
    [2, -6],
    [4, -4],
  ] as const) {
    ctx.block(x, 4, z, PAL.grassDark);
    addGrassTuft(ctx, x, z, 5, 0.55);
  }

  for (const x of [-2, 2]) {
    ctx.detail(x - 0.1, 1, 0.02, 0.24, 3.3, 0.24, PAL.planksDark);
    ctx.detail(x - 0.5, 4.0, 0.02, 1.1, 0.24, 0.3, PAL.planksDark);
    ctx.detail(
      x + (x < 0 ? 0.2 : -0.9),
      2.7,
      0.04,
      0.7,
      0.14,
      0.18,
      PAL.planksDark,
    );
  }
  ctx.detail(-2.5, 4.22, 0.02, 5.02, 0.22, 0.32, PAL.planks);
  ctx.detail(-1.4, 3.94, 0.1, 2.8, 0.12, 0.24, PAL.planksDark);

  addRails(ctx, 0, -4, 1, 8);
  addMineCart(ctx, 0, 1, 2.4);

  addLantern(ctx, -2.7, 0.6, 0.7);
  addLantern(ctx, 2.7, -0.3, 3.1);

  addCrystalCluster(ctx, center(-4, 1, 1)[0], 0.34, center(-4, 1, 1)[2], 0.9);
  addCrystalCluster(ctx, center(4, 1, -2)[0], 0.34, center(4, 1, -2)[2], 0.65);

  addPine(ctx, -3, -4, 5, 3);
  addPine(ctx, 3, -5, 5, 4);
  ctx.block(-5, 5, -3, PAL.stoneDark);
  ctx.block(-5, 5, -2, PAL.stone);
  ctx.block(4, 5, -3, PAL.stoneDark);
  ctx.block(4, 5, -2, PAL.stone);
  ctx.block(1, 5, -5, PAL.stoneDark);

  ctx.movers.push((time) => {
    tunnelMist.material.opacity =
      0.1 + (0.5 + 0.5 * Math.sin(time * 0.6)) * 0.05;
  });

  ctx.water(0.55, 0.6, [
    center(1.6, 1, -0.2)[0],
    0.356,
    center(1.6, 1, -0.2)[2],
  ]);
  ctx.ripple(
    center(1.6, 1, -0.2)[0],
    center(1.6, 1, -0.2)[2],
    0.358,
    "#c9fff2",
    0.3,
    0,
    0.8,
  );

  const dropGeometry = new SphereGeometry(0.035, 6, 4);
  const dropMaterial = new MeshBasicMaterial({
    color: "#c9fff2",
    transparent: true,
    opacity: 0.85,
    toneMapped: false,
  });
  ctx.resources.add(dropGeometry);
  ctx.resources.add(dropMaterial);
  const drop = new Mesh(dropGeometry, dropMaterial);
  ctx.group.add(drop);
  ctx.hidden.push(drop);
  const dropPoint = corner(1.6, 1, -0.2);
  ctx.movers.push((time) => {
    const life = fract(time * 0.38);
    drop.position.set(dropPoint[0], 1.35 - life * 1, dropPoint[2]);
    drop.visible = life < 0.96;
    dropMaterial.opacity = 0.85 * (1 - life * 0.4);
  });

  ctx.particles({
    count: 26,
    motion: "dust",
    colors: ["#bfe8dd", "#9dbdb5", "#d9fff4"],
    origin: [0, 1.05, -1.2],
    spread: [3.2, 1.6, 2.6],
    size: 4,
    sizeJitter: 0.6,
    speed: 0.03,
    opacity: 0.35,
    additive: true,
  });

  for (const [x, z, color] of [
    [-5, 3, "#d98d78"],
    [5, 2, "#c9a7d9"],
    [-6, -1, "#d9c07a"],
  ] as const) {
    addFlower(ctx, x, z, 1, color);
  }
}
