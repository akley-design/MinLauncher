import {
  BoxGeometry,
  DoubleSide,
  Group,
  Mesh,
  MeshBasicMaterial,
  PlaneGeometry,
} from "three";
import {
  addBirch,
  addBoat,
  addBush,
  addCottage,
  addFence,
  addFlower,
  addGrassTuft,
  addOak,
  center,
  petal,
  SceneBuilder,
} from "../kit";
import { PAL } from "../palette";
import { hash3 } from "../motion";

function addButterfly(
  ctx: SceneBuilder,
  cx: number,
  cy: number,
  cz: number,
  color: string,
  phase: number,
) {
  const group = new Group();
  group.position.set(cx, cy, cz);
  ctx.group.add(group);

  const wingGeometry = new PlaneGeometry(0.17, 0.11);
  const wingMaterial = new MeshBasicMaterial({
    color,
    side: DoubleSide,
    toneMapped: false,
  });
  const bodyGeometry = new BoxGeometry(0.05, 0.03, 0.14);
  const bodyMaterial = new MeshBasicMaterial({
    color: "#41453c",
    toneMapped: false,
  });
  ctx.resources.add(wingGeometry);
  ctx.resources.add(wingMaterial);
  ctx.resources.add(bodyGeometry);
  ctx.resources.add(bodyMaterial);

  const leftPivot = new Group();
  const rightPivot = new Group();
  const left = new Mesh(wingGeometry, wingMaterial);
  left.rotation.x = -Math.PI / 2;
  left.position.x = -0.09;
  const right = new Mesh(wingGeometry, wingMaterial);
  right.rotation.x = -Math.PI / 2;
  right.position.x = 0.09;
  leftPivot.add(left);
  rightPivot.add(right);
  group.add(leftPivot, rightPivot, new Mesh(bodyGeometry, bodyMaterial));

  ctx.hidden.push(group);
  ctx.movers.push((time) => {
    const angle = time * 0.55 + phase;
    group.position.set(
      cx + Math.sin(angle) * 0.85,
      cy + Math.sin(angle * 1.7) * 0.18,
      cz + Math.sin(angle * 2) * 0.5,
    );
    group.rotation.y = angle;
    const flap = 0.35 + Math.sin(time * 13 + phase) * 0.55;
    leftPivot.rotation.z = flap;
    rightPivot.rotation.z = -flap;
  });
}

export function buildIsland(ctx: SceneBuilder) {
  const inside = (x: number, z: number, shrink = 0) => {
    const nx = (x + 0.5) / (8.3 - shrink);
    const nz = (z + 0.5) / (7.2 - shrink);
    return nx * nx + nz * nz <= 1;
  };

  for (let x = -8; x <= 7; x++) {
    for (let z = -7; z <= 6; z++) {
      if (!inside(x, z)) continue;
      const river = x === 2 || x === 3;
      for (let level = 0; level <= 6; level++) {
        if (level > 0 && !inside(x, z, 0.7 + level * 0.82)) break;
        if (level === 0) {
          if (river) continue;
          const pick = hash3(x, z, 3);
          const style =
            pick > 0.72
              ? PAL.grassLime
              : pick > 0.28
                ? PAL.grass
                : PAL.grassDark;
          ctx.block(x, 0, z, style);
        } else {
          const pick = hash3(x, z, level * 7);
          const style =
            level <= 2
              ? pick > 0.5
                ? PAL.dirt
                : PAL.dirtDark
              : level <= 5
                ? pick > 0.5
                  ? PAL.stone
                  : PAL.stoneDark
                : PAL.deepslate;
          ctx.block(x, -level, z, style);
        }
      }
    }
  }

  for (let z = -7; z <= 6; z++) {
    for (const x of [2, 3]) {
      if (!inside(x, z)) continue;
      ctx.block(x, -1, z, z % 3 === 0 ? PAL.gravel : PAL.sand);
      ctx.block(x, -2, z, PAL.dirtDark);
    }
    if (inside(1, z) && z % 2 === 0) ctx.block(1, 0, z, PAL.sand);
    if (inside(4, z) && z % 3 === 0) ctx.block(4, 0, z, PAL.gravel);
  }

  for (let z = 1; z <= 5; z++) {
    ctx.block(-3, 0, z, z % 2 ? PAL.cobble : PAL.gravel);
    ctx.block(-2, 0, z, z % 2 ? PAL.gravel : PAL.cobble);
  }
  ctx.detail(-3.4, 0.02, 0.9, 2.8, 0.06, 1.1, PAL.cobbleDark);

  const cottage = addCottage(ctx, -5, 0, -4);
  const windowLight = ctx.light("#ffc98a", 1.35, 4, cottage.windowLight);
  const windowGlow = ctx.glow("#ffd094", cottage.windowLight, 0.72, 0.22);
  ctx.movers.push((time) => {
    const breath = 0.5 + 0.5 * Math.sin(time * 1.05);
    windowLight.intensity = 1.05 + breath * 0.5;
    windowGlow.material.opacity = 0.16 + breath * 0.16;
  });

  ctx.particles({
    count: 16,
    motion: "smoke",
    colors: ["#c8cec0", "#b6c0b1", "#d7dcd1"],
    origin: cottage.smoke,
    spread: [0.12, 0.1, 0.12],
    size: 16,
    sizeJitter: 0.6,
    speed: 0.055,
    opacity: 0.3,
  });

  addOak(ctx, 5, -3, 1, "large");
  addBirch(ctx, -6, 2, 1);
  addBush(ctx, 0, 3, 1, PAL.leavesLime);

  const groundY = 0.346;
  ctx.patch(
    center(-2.5, 0, -2)[0],
    center(-2.5, 0, -2)[2],
    groundY,
    2.5,
    2.2,
    0.3,
  );
  ctx.patch(center(5, 0, -3)[0], center(5, 0, -3)[2], groundY, 1.6, 1.6, 0.22);
  ctx.patch(center(-6, 0, 2)[0], center(-6, 0, 2)[2], groundY, 1.25, 1.25, 0.2);

  addFence(ctx, -1, 1, 1, 4, "z");
  for (const [x, z, color] of [
    [-6, 3, "#d98d78"],
    [-6, 4, "#d9c07a"],
    [-4, 3, "#c9a7d9"],
    [-1, 4, "#d98d78"],
    [0, 2, "#d9c07a"],
    [4, -1, "#d98d78"],
    [5, 1, "#c9a7d9"],
  ] as const) {
    addFlower(ctx, x, z, 1, color);
  }
  for (const [x, z] of [
    [-7, 1],
    [-4, 2],
    [-2, 4],
    [1, 5],
    [4, 4],
    [6, 0],
    [6, -5],
    [1, -4],
  ] as const) {
    addGrassTuft(ctx, x, z, 1, 0.5 + hash3(x, z, 13) * 0.4);
  }
  for (const z of [3, 4]) {
    ctx.detail(0.7, 0.03, z + 0.1, 0.9, 0.12, 0.8, PAL.planksDark);
  }
  ctx.detail(-4.8, 1, 1.2, 0.7, 0.7, 0.7, PAL.planksDark);
  ctx.detail(-4, 1, 1.3, 0.55, 0.55, 0.55, PAL.logDark);

  addBoat(ctx, 2.5, 0.42, 2.4, Math.PI / 2 + 0.12);

  for (const [x, z] of [
    [2.2, 1.2],
    [3.2, 0.2],
    [2.6, -1.6],
    [3.1, -3.2],
  ] as const) {
    ctx.detail(x + 0.1, 0.38, z + 0.1, 0.55, 0.05, 0.55, petal("#6f8f5e"));
  }
  for (const [x, z] of [
    [1, 2],
    [1, 4],
    [4, -2],
    [4, 1],
  ] as const) {
    addGrassTuft(ctx, x, z, 1, 0.7);
  }

  const riverX = 1.02;
  ctx.water(
    0.94,
    4.5,
    [riverX, 0.135, 0.1],
    -Math.PI / 2,
    "#3f8f88",
    "#b8eee4",
    0.58,
  );
  ctx.waterfall(0.8, 1.35, [riverX, -0.28, 2.41], "#3f948a", "#a9e5d9", 0.45);
  const mist = ctx.glow("#b8eee4", [riverX, -0.55, 2.43], 1.1, 0.08);
  ctx.movers.push((time) => {
    mist.material.opacity = 0.05 + (0.5 + 0.5 * Math.sin(time * 0.8)) * 0.05;
  });
  ctx.particles({
    count: 34,
    motion: "spray",
    colors: ["#e6fff8", "#bff3e8", "#d7fff4"],
    origin: [riverX, -0.95, 2.42],
    spread: [0.7, 0.2, 0.2],
    size: 5,
    sizeJitter: 0.5,
    speed: 0.34,
    opacity: 0.5,
    additive: true,
  });
  ctx.ripple(riverX, 1.4, 0.142, "#d8fff2", 0.2, 0.1, 1);
  ctx.ripple(riverX, 0.2, 0.142, "#d8fff2", 0.2, 0.45, 1);
  ctx.ripple(riverX, -1.1, 0.142, "#d8fff2", 0.2, 0.8, 1);

  ctx.particles({
    count: 20,
    motion: "firefly",
    colors: ["#ffe7a6", "#d9f38f", "#fff3c4"],
    origin: center(-3, 1, 2),
    spread: [3.2, 1.2, 2.4],
    size: 7,
    sizeJitter: 0.5,
    speed: 0.07,
    opacity: 0.8,
    additive: true,
  });
  ctx.particles({
    count: 12,
    motion: "leaf",
    colors: ["#8fae6d", "#c9956c", "#6f8f5e"],
    origin: center(5, 6, -3),
    spread: [2, 0.6, 2],
    size: 4,
    sizeJitter: 0.6,
    speed: 0.055,
    opacity: 0.8,
  });

  addButterfly(ctx, -1.35, 1.05, 0.95, "#e8c477", 0.4);
  addButterfly(ctx, 0.25, 1.15, 1.5, "#d9a0c4", 2.1);
  addButterfly(ctx, 1.5, 1.25, -0.1, "#cfe08a", 4.2);
}
