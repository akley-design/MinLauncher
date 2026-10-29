import {
  AdditiveBlending,
  ConeGeometry,
  CylinderGeometry,
  DoubleSide,
  Group,
  Mesh,
  MeshBasicMaterial,
  MeshStandardMaterial,
  Object3D,
  PlaneGeometry,
  PointLight,
  RingGeometry,
  Sprite,
  SpriteMaterial,
  Vector3,
} from "three";
import type { DataTexture, IUniform } from "three";
import { PAL } from "./palette";
import {
  createGlowTexture,
  createParticleField,
  type ParticleField,
  type ParticleOptions,
} from "./particles";
import { ResourceRegistry } from "./resources";
import { createWaterfallMaterial, createWaterMaterial } from "./shaders";
import { TextureBank } from "./textures";
import { buildVoxelMeshes, VOXEL_SIZE, VoxelBuilder } from "./voxel";
import type { BlockStyle } from "./voxel";
import { fract, wind } from "./motion";

export const S = VOXEL_SIZE;

export function center(
  x: number,
  y: number,
  z: number,
): [number, number, number] {
  return [(x + 0.5) * S, (y + 0.5) * S, (z + 0.5) * S];
}

export function corner(
  x: number,
  y: number,
  z: number,
): [number, number, number] {
  return [x * S, y * S, z * S];
}

type NumberUniform = IUniform<number>;

export class SceneBuilder {
  readonly group = new Group();
  readonly voxel = new VoxelBuilder();
  readonly bank: TextureBank;
  readonly resources = new ResourceRegistry();
  readonly movers: Array<(time: number) => void> = [];
  readonly hidden: Object3D[] = [];
  readonly timeUniforms: NumberUniform[] = [];
  readonly pixelUniforms: NumberUniform[] = [];
  private glowTexture: DataTexture | null = null;

  constructor(anisotropy = 1) {
    this.group.name = "ml-site-world";
    this.bank = new TextureBank(anisotropy);
    this.resources.add(this.bank);
  }

  block(x: number, y: number, z: number, style: BlockStyle, tint?: string) {
    this.voxel.set(x, y, z, style, tint);
    return this;
  }

  fill(
    x: number,
    y: number,
    z: number,
    width: number,
    height: number,
    depth: number,
    style: BlockStyle,
    tint?: string,
  ) {
    this.voxel.fill(x, y, z, width, height, depth, style, tint);
    return this;
  }

  detail(
    x: number,
    y: number,
    z: number,
    width: number,
    height: number,
    depth: number,
    style: BlockStyle,
    tint?: string,
  ) {
    this.voxel.detail(x, y, z, width, height, depth, style, tint);
    return this;
  }

  finish() {
    const result = buildVoxelMeshes(this.voxel, this.bank, this.resources);
    this.group.add(result.group);
    return result;
  }

  part(
    pivot: [number, number, number],
    build: (builder: VoxelBuilder) => void,
    parent: Group = this.group,
  ) {
    const builder = new VoxelBuilder();
    build(builder);
    const result = buildVoxelMeshes(builder, this.bank, this.resources, {
      x: pivot[0],
      y: pivot[1],
      z: pivot[2],
    });
    result.group.position.set(pivot[0], pivot[1], pivot[2]);
    parent.add(result.group);
    return result;
  }

  private ensureGlow() {
    if (!this.glowTexture) {
      this.glowTexture = createGlowTexture();
      this.resources.add(this.glowTexture);
    }
    return this.glowTexture;
  }

  glow(
    color: string,
    position: [number, number, number],
    scale: number,
    opacity = 0.8,
  ) {
    const material = new SpriteMaterial({
      map: this.ensureGlow(),
      color,
      transparent: true,
      blending: AdditiveBlending,
      depthWrite: false,
      opacity,
      toneMapped: false,
    });
    this.resources.add(material);
    const sprite = new Sprite(material);
    sprite.position.set(...position);
    sprite.scale.setScalar(scale);
    sprite.renderOrder = 5;
    this.group.add(sprite);
    return { sprite, material };
  }

  pool(
    color: string,
    position: [number, number, number],
    scale: number,
    opacity = 0.4,
  ) {
    const geometry = new PlaneGeometry(1, 1);
    const material = new MeshBasicMaterial({
      map: this.ensureGlow(),
      color,
      transparent: true,
      blending: AdditiveBlending,
      depthWrite: false,
      opacity,
      toneMapped: false,
      side: DoubleSide,
    });
    this.resources.add(geometry);
    this.resources.add(material);
    const mesh = new Mesh(geometry, material);
    mesh.rotation.x = -Math.PI / 2;
    mesh.position.set(...position);
    mesh.scale.set(scale, scale, 1);
    mesh.renderOrder = 4;
    this.group.add(mesh);
    return { mesh, material };
  }

  patch(
    x: number,
    z: number,
    y: number,
    scaleX: number,
    scaleZ: number,
    opacity = 0.3,
  ) {
    const geometry = new PlaneGeometry(1, 1);
    const material = new MeshBasicMaterial({
      map: this.ensureGlow(),
      color: 0x070a08,
      transparent: true,
      depthWrite: false,
      opacity,
      toneMapped: false,
      side: DoubleSide,
    });
    this.resources.add(geometry);
    this.resources.add(material);
    const mesh = new Mesh(geometry, material);
    mesh.rotation.x = -Math.PI / 2;
    mesh.position.set(x, y, z);
    mesh.scale.set(scaleX, scaleZ, 1);
    mesh.renderOrder = 1;
    this.group.add(mesh);
    return mesh;
  }

  particles(options: ParticleOptions): ParticleField {
    const field = createParticleField(options);
    this.resources.add(field);
    this.timeUniforms.push(field.material.uniforms.uTime!);
    this.pixelUniforms.push(field.material.uniforms.uPixelRatio!);
    this.hidden.push(field.object);
    this.group.add(field.object);
    return field;
  }

  water(
    width: number,
    length: number,
    position: [number, number, number],
    rotationX = -Math.PI / 2,
    color = "#5fb8ac",
    highlight = "#d4fff2",
    opacity = 0.72,
  ) {
    const material = createWaterMaterial(color, highlight, opacity);
    const geometry = new PlaneGeometry(width, length, 1, 6);
    this.resources.add(geometry);
    this.resources.add(material);
    this.timeUniforms.push(material.uniforms.uTime!);
    const mesh = new Mesh(geometry, material);
    mesh.rotation.x = rotationX;
    mesh.position.set(...position);
    mesh.renderOrder = 2;
    this.group.add(mesh);
    return mesh;
  }

  waterfall(
    width: number,
    height: number,
    position: [number, number, number],
    color = "#7fd3c6",
    highlight = "#e0fff8",
    opacity = 0.66,
  ) {
    const material = createWaterfallMaterial(color, highlight, opacity);
    const geometry = new PlaneGeometry(width, height, 1, 1);
    this.resources.add(geometry);
    this.resources.add(material);
    this.timeUniforms.push(material.uniforms.uTime!);
    const mesh = new Mesh(geometry, material);
    mesh.position.set(...position);
    mesh.renderOrder = 3;
    this.group.add(mesh);
    return mesh;
  }

  ripple(
    x: number,
    z: number,
    y: number,
    color = "#d8fff2",
    speed = 0.22,
    phase = 0,
    baseScale = 1,
  ) {
    const geometry = new RingGeometry(0.055, 0.082, 24);
    const material = new MeshBasicMaterial({
      color,
      transparent: true,
      opacity: 0.45,
      depthWrite: false,
      side: DoubleSide,
      toneMapped: false,
    });
    this.resources.add(geometry);
    this.resources.add(material);
    const mesh = new Mesh(geometry, material);
    mesh.rotation.x = -Math.PI / 2;
    mesh.position.set(x, y, z);
    mesh.renderOrder = 3;
    this.group.add(mesh);
    this.hidden.push(mesh);
    this.movers.push((time) => {
      const life = fract(time * speed + phase);
      mesh.scale.setScalar(baseScale * (0.7 + life * 2.6));
      material.opacity = (1 - life) * 0.4;
    });
    return mesh;
  }

  light(
    color: string,
    intensity: number,
    distance: number,
    position: [number, number, number],
  ) {
    const light = new PointLight(color, intensity, distance, 2);
    light.position.set(...position);
    this.group.add(light);
    return light;
  }

  mover(fn: (time: number) => void) {
    this.movers.push(fn);
  }

  hide(object: Object3D) {
    this.hidden.push(object);
  }

  setPixelRatio(ratio: number) {
    for (const uniform of this.pixelUniforms) uniform.value = ratio;
  }
}

export function petal(color: string): BlockStyle {
  return { top: "leaves", side: "leaves", bottom: "leaves", tint: color };
}

export function addOak(
  ctx: SceneBuilder,
  x: number,
  z: number,
  groundY: number,
  size: "large" | "small" = "large",
) {
  const trunkHeight = size === "large" ? 4 : 3;
  ctx.voxel.fill(x, groundY, z, 1, trunkHeight, 1, PAL.log);
  const layers = size === "large" ? [5, 5, 3, 1] : [3, 3, 1];
  layers.forEach((span, layer) => {
    const y = groundY + trunkHeight - 1 + layer;
    const pivot = center(x, y, z);
    const part = ctx.part(pivot, (builder) => {
      const half = Math.floor(span / 2);
      for (let dx = -half; dx <= half; dx++) {
        for (let dz = -half; dz <= half; dz++) {
          if (span > 1 && Math.abs(dx) === half && Math.abs(dz) === half)
            continue;
          if (layer === 0 && dx === 0 && dz === 0) continue;
          const tint =
            (dx + dz + layer) % 3 === 0 ? PAL.leavesDark : PAL.leaves;
          builder.set(x + dx, y, z + dz, tint);
        }
      }
      if (layer === 0) {
        for (let dx = -1; dx <= 1; dx++)
          for (let dz = -1; dz <= 1; dz++)
            if (dx !== 0 || dz !== 0)
              builder.set(x + dx, y - 1, z + dz, PAL.leavesDark);
      }
      if (layer === 1) {
        builder.set(x, y + 1, z, PAL.leavesLime);
      }
    });
    const phase = layer * 1.3 + x * 0.2 + z * 0.15;
    ctx.movers.push((time) => {
      const gust = wind(time, x, z);
      part.group.rotation.z = Math.sin(time * 0.6 + phase) * 0.022 * gust;
      part.group.rotation.x =
        Math.sin(time * 0.45 + phase * 0.8) * 0.016 * gust;
    });
  });
  return center(x, groundY + trunkHeight + layers.length, z);
}

export function addBirch(
  ctx: SceneBuilder,
  x: number,
  z: number,
  groundY: number,
) {
  const trunkHeight = 3;
  ctx.voxel.fill(x, groundY, z, 1, trunkHeight, 1, PAL.birch);
  const layers = [3, 3, 1];
  layers.forEach((span, layer) => {
    const y = groundY + trunkHeight - 1 + layer;
    const pivot = center(x, y, z);
    const part = ctx.part(pivot, (builder) => {
      const half = Math.floor(span / 2);
      for (let dx = -half; dx <= half; dx++) {
        for (let dz = -half; dz <= half; dz++) {
          if (span > 1 && Math.abs(dx) === half && Math.abs(dz) === half)
            continue;
          if (layer === 0 && dx === 0 && dz === 0) continue;
          builder.set(x + dx, y, z + dz, PAL.leavesBirch);
        }
      }
      if (layer === 0) builder.set(x, y - 1, z + 1, PAL.leavesBirch);
    });
    const phase = layer + x * 0.3;
    ctx.movers.push((time) => {
      const gust = wind(time, x, z);
      part.group.rotation.z = Math.sin(time * 0.7 + phase) * 0.025 * gust;
      part.group.rotation.x = Math.sin(time * 0.5 + phase) * 0.018 * gust;
    });
  });
}

export function addPine(
  ctx: SceneBuilder,
  x: number,
  z: number,
  groundY: number,
  height = 5,
) {
  ctx.voxel.fill(x, groundY, z, 1, height - 1, 1, PAL.logDark);
  const layers: Array<[number, number]> = [
    [height - 3, 3],
    [height - 2, 3],
    [height - 1, 1],
  ];
  for (const [offset, span] of layers) {
    const y = groundY + offset;
    const pivot = center(x, y, z);
    const part = ctx.part(pivot, (builder) => {
      const half = Math.floor(span / 2);
      for (let dx = -half; dx <= half; dx++)
        for (let dz = -half; dz <= half; dz++) {
          if (span > 1 && Math.abs(dx) === half && Math.abs(dz) === half)
            continue;
          if (span > 1 && dx === 0 && dz === 0) continue;
          builder.set(x + dx, y, z + dz, PAL.leavesPine);
        }
    });
    ctx.movers.push((time) => {
      part.group.rotation.z = Math.sin(time * 0.55 + offset) * 0.018;
      part.group.rotation.x = Math.sin(time * 0.4 + offset) * 0.012;
    });
  }
}

export function addBush(
  ctx: SceneBuilder,
  x: number,
  z: number,
  groundY: number,
  style: BlockStyle = PAL.leavesDark,
) {
  ctx.voxel.fill(x, groundY, z, 2, 1, 2, style);
  ctx.voxel.set(x, groundY + 1, z + 1, style);
  ctx.voxel.set(x + 1, groundY + 1, z, style);
}

export function addFlower(
  ctx: SceneBuilder,
  x: number,
  z: number,
  groundY: number,
  color: string,
) {
  ctx.voxel.detail(
    x + 0.42,
    groundY,
    z + 0.42,
    0.16,
    0.34,
    0.16,
    PAL.leavesLime,
  );
  ctx.voxel.detail(
    x + 0.24,
    groundY + 0.3,
    z + 0.24,
    0.5,
    0.26,
    0.5,
    petal(color),
  );
}

export function addGrassTuft(
  ctx: SceneBuilder,
  x: number,
  z: number,
  groundY: number,
  height = 0.55,
) {
  ctx.voxel.detail(
    x + 0.4,
    groundY,
    z + 0.4,
    0.06,
    height,
    0.06,
    PAL.leavesLime,
  );
  ctx.voxel.detail(
    x + 0.55,
    groundY,
    z + 0.35,
    0.05,
    height * 0.8,
    0.05,
    PAL.leaves,
  );
  ctx.voxel.detail(
    x + 0.3,
    groundY,
    z + 0.6,
    0.05,
    height * 0.65,
    0.05,
    PAL.leavesDark,
  );
}

export interface CottageAnchors {
  smoke: [number, number, number];
  windowLight: [number, number, number];
  lantern: [number, number, number];
}

export function addCottage(
  ctx: SceneBuilder,
  x: number,
  y: number,
  z: number,
): CottageAnchors {
  ctx.voxel.fill(x, y, z, 6, 1, 5, PAL.cobble);

  for (let yy = y + 1; yy <= y + 3; yy++) {
    for (let ix = 0; ix < 6; ix++) {
      for (let iz = 0; iz < 5; iz++) {
        const front = iz === 4;
        const border = front || iz === 0 || ix === 0 || ix === 5;
        if (!border) continue;
        if (front && ix === 2 && yy <= y + 2) continue;
        const post =
          (ix === 0 || ix === 5) && (iz === 0 || iz === 4)
            ? true
            : front && (ix === 2 || ix === 3);
        const window =
          yy === y + 2 &&
          ((front && (ix === 1 || ix === 4)) ||
            ((ix === 0 || ix === 5) && iz === 2));
        const style = window
          ? PAL.window
          : post
            ? PAL.planksDark
            : yy === y + 3
              ? PAL.plasterWarm
              : PAL.plaster;
        ctx.voxel.set(x + ix, yy, z + iz, style);
      }
    }
  }

  for (const yy of [y + 1, y + 3]) {
    ctx.voxel.detail(
      x + 0.04,
      yy + 0.03,
      z + 5.02,
      5.92,
      0.1,
      0.06,
      PAL.planksDark,
    );
    ctx.voxel.detail(
      x - 0.08,
      yy + 0.03,
      z + 0.04,
      0.06,
      0.1,
      4.92,
      PAL.planksDark,
    );
    ctx.voxel.detail(
      x + 6.02,
      yy + 0.03,
      z + 0.04,
      0.06,
      0.1,
      4.92,
      PAL.planksDark,
    );
  }

  ctx.voxel.detail(
    x + 1.56,
    y + 1.02,
    z + 5.03,
    0.92,
    1.72,
    0.08,
    PAL.planksDark,
  );
  ctx.voxel.detail(
    x + 2.34,
    y + 1.2,
    z + 5.11,
    0.14,
    0.14,
    0.06,
    petal("#d9c07a"),
  );
  ctx.voxel.detail(x + 1.4, y + 0.96, z + 5.55, 1.3, 0.14, 0.5, PAL.cobbleDark);

  for (const ix of [1, 4]) {
    ctx.voxel.detail(x + ix, y + 2, z + 5.03, 0.08, 1, 0.07, PAL.planksDark);
    ctx.voxel.detail(
      x + ix + 0.92,
      y + 2,
      z + 5.03,
      0.08,
      1,
      0.07,
      PAL.planksDark,
    );
    ctx.voxel.detail(x + ix, y + 2.92, z + 5.03, 1, 0.08, 0.07, PAL.planksDark);
    ctx.voxel.detail(
      x + ix + 0.05,
      y + 1.9,
      z + 5.1,
      0.9,
      0.16,
      0.3,
      PAL.planksDark,
    );
    ctx.voxel.detail(
      x + ix + 0.18,
      y + 2.06,
      z + 5.2,
      0.12,
      0.2,
      0.12,
      PAL.leavesLime,
    );
    ctx.voxel.detail(
      x + ix + 0.5,
      y + 2.08,
      z + 5.2,
      0.24,
      0.22,
      0.2,
      petal("#d98d78"),
    );
    ctx.voxel.detail(
      x + ix + 0.74,
      y + 2.06,
      z + 5.2,
      0.12,
      0.18,
      0.12,
      PAL.leavesLime,
    );
  }

  ctx.voxel.detail(x + 0.9, y + 1, z + 5.5, 0.18, 2.1, 0.18, PAL.planksDark);
  ctx.voxel.detail(x + 3.7, y + 1, z + 5.5, 0.18, 2.1, 0.18, PAL.planksDark);
  ctx.voxel.detail(x + 0.7, y + 3.02, z + 5.3, 3.2, 0.16, 0.8, PAL.planksDark);

  const roofLayers: Array<[number, number, number, BlockStyle]> = [
    [y + 4, -1, 7, PAL.roof],
    [y + 5, 0, 5, PAL.roofDark],
    [y + 6, 1, 3, PAL.roof],
    [y + 7, 2, 1, PAL.roofDark],
  ];
  for (const [yy, zInset, depth, style] of roofLayers) {
    ctx.voxel.fill(x - 1, yy, z + zInset, 8, 1, depth, style);
  }
  ctx.voxel.detail(x - 1, y + 7.95, z + 2.42, 8, 0.14, 0.14, PAL.planksDark);

  ctx.voxel.detail(x + 2, y + 5.2, z + 5.02, 1, 0.86, 0.07, PAL.window);
  ctx.voxel.detail(
    x + 1.96,
    y + 5.14,
    z + 5.0,
    0.08,
    0.98,
    0.1,
    PAL.planksDark,
  );
  ctx.voxel.detail(
    x + 2.96,
    y + 5.14,
    z + 5.0,
    0.08,
    0.98,
    0.1,
    PAL.planksDark,
  );
  ctx.voxel.detail(
    x + 1.96,
    y + 5.98,
    z + 5.0,
    1.08,
    0.08,
    0.1,
    PAL.planksDark,
  );
  ctx.voxel.detail(
    x + 1.96,
    y + 5.12,
    z + 5.0,
    1.08,
    0.08,
    0.1,
    PAL.planksDark,
  );

  ctx.voxel.fill(x + 1, y + 5, z + 1, 1, 4, 1, PAL.cobble);
  ctx.voxel.detail(x + 0.9, y + 8.98, z + 0.9, 1.2, 0.2, 1.2, PAL.cobbleDark);

  ctx.voxel.detail(x - 0.62, y + 1, z + 5.42, 0.22, 2.2, 0.22, PAL.planksDark);
  ctx.voxel.detail(x - 0.78, y + 3.1, z + 5.26, 0.54, 0.34, 0.54, PAL.lantern);
  const lanternWorld = center(x - 0.55, y + 3.25, z + 5.55);
  ctx.glow("#ffd79a", lanternWorld, 0.8, 0.34);
  ctx.light("#ffc47d", 1.3, 3.2, [
    lanternWorld[0],
    lanternWorld[1],
    lanternWorld[2],
  ]);

  const smoke = center(x + 1, y + 9.25, z + 1);
  const windowLight = center(x + 2.5, y + 2.5, z + 5.5);
  return { smoke, windowLight, lantern: lanternWorld };
}

export function addFence(
  ctx: SceneBuilder,
  x: number,
  z: number,
  groundY: number,
  length: number,
  axis: "x" | "z",
) {
  for (let step = 0; step <= length; step += 2) {
    const px = axis === "x" ? x + step : x;
    const pz = axis === "z" ? z + step : z;
    ctx.voxel.detail(
      px + 0.42,
      groundY,
      pz + 0.42,
      0.16,
      1.15,
      0.16,
      PAL.planksDark,
    );
  }
  for (const railY of [0.35, 0.75]) {
    ctx.voxel.detail(
      x + 0.1,
      groundY + railY,
      z + 0.46,
      axis === "x" ? length + 0.8 : 0.12,
      0.12,
      axis === "z" ? length + 0.8 : 0.12,
      PAL.planks,
    );
  }
}

export function addBoat(
  ctx: SceneBuilder,
  x: number,
  y: number,
  z: number,
  angle: number,
) {
  const pivot = center(x, y, z);
  const part = ctx.part(pivot, (builder) => {
    builder.detail(x - 0.4, y, z - 0.5, 3.2, 0.24, 1.1, PAL.planksDark);
    builder.detail(x - 0.2, y + 0.22, z - 0.38, 2.8, 0.16, 0.86, PAL.planks);
    builder.detail(x - 0.4, y + 0.36, z - 0.55, 3.2, 0.3, 0.12, PAL.planks);
    builder.detail(x - 0.4, y + 0.36, z + 0.43, 3.2, 0.3, 0.12, PAL.planks);
    builder.detail(x + 1.1, y + 0.3, z - 0.3, 0.7, 0.12, 0.6, PAL.planksPale);
  });
  part.group.rotation.y = angle;
  ctx.movers.push((time) => {
    part.group.position.y = pivot[1] + Math.sin(time * 0.9 + x) * 0.028;
    part.group.rotation.z = Math.sin(time * 0.7 + z) * 0.035;
    part.group.rotation.x = Math.sin(time * 0.5 + x * 0.4) * 0.02;
  });
  return part.group;
}

export function addCrystalCluster(
  ctx: SceneBuilder,
  x: number,
  y: number,
  z: number,
  scale = 1,
) {
  const material = new MeshStandardMaterial({
    color: "#2f9e87",
    emissive: "#1a5f55",
    emissiveIntensity: 0.25,
    roughness: 0.62,
    metalness: 0.04,
    flatShading: true,
  });
  ctx.resources.add(material);
  const group = new Group();
  group.position.set(x, y, z);
  ctx.group.add(group);

  const shards: Array<[number, number, number, number, number]> = [
    [0, 0, 0.36, 0.95, 0.1],
    [-0.26, 0.1, 0.26, 0.68, -0.3],
    [0.28, -0.06, 0.2, 0.55, 0.4],
    [0.06, 0.24, 0.16, 0.42, 0.18],
  ];
  for (const [dx, dz, radius, height, tilt] of shards) {
    const geometry = new ConeGeometry(radius * scale, height * scale, 5);
    ctx.resources.add(geometry);
    const mesh = new Mesh(geometry, material);
    mesh.position.set(dx * scale, (height * scale) / 2, dz * scale);
    mesh.rotation.z = tilt;
    group.add(mesh);
  }

  const glow = ctx.glow(
    "#7df0d2",
    [x, y + 0.36 * scale, z],
    0.95 * scale,
    0.24,
  );
  const light = ctx.light("#6fe8c9", 0.85, 2.5, [x, y + 0.4 * scale, z]);
  ctx.movers.push((time) => {
    const pulse = 0.5 + 0.5 * Math.sin(time * 1.15 + x * 0.7);
    material.emissiveIntensity = 0.25 + pulse * 0.2;
    glow.material.opacity = 0.13 + pulse * 0.08;
    light.intensity = 0.5 + pulse * 0.25;
  });
  return group;
}

export function addMineCart(
  ctx: SceneBuilder,
  x: number,
  y: number,
  z: number,
) {
  const pivot = center(x, y, z);
  const part = ctx.part(pivot, (builder) => {
    builder.detail(x - 0.8, y + 0.3, z - 0.5, 1.6, 0.55, 1, PAL.metal);
    builder.detail(x - 0.7, y + 0.84, z - 0.4, 1.4, 0.35, 0.8, PAL.metal);
    builder.detail(x - 0.5, y + 0.9, z - 0.25, 0.38, 0.26, 0.38, PAL.oreCopper);
    builder.detail(
      x + 0.15,
      y + 0.92,
      z - 0.2,
      0.32,
      0.24,
      0.32,
      PAL.oreAmethyst,
    );
  });
  part.group.rotation.y = 0.08;

  const wheelGeometry = new CylinderGeometry(0.16, 0.16, 0.1, 10);
  wheelGeometry.rotateZ(Math.PI / 2);
  ctx.resources.add(wheelGeometry);
  const wheelMaterial = new MeshStandardMaterial({
    color: "#4d5450",
    roughness: 0.55,
    metalness: 0.55,
  });
  ctx.resources.add(wheelMaterial);
  const wheels: Mesh[] = [];
  for (const dx of [-0.22, 0.22]) {
    for (const dz of [-0.12, 0.12]) {
      const wheel = new Mesh(wheelGeometry, wheelMaterial);
      wheel.position.set(dx, -0.1, dz);
      wheel.castShadow = true;
      part.group.add(wheel);
      wheels.push(wheel);
    }
  }
  ctx.movers.push((time) => {
    part.group.position.z = pivot[2] + Math.sin(time * 0.32) * 0.03;
    part.group.rotation.x = Math.sin(time * 0.6) * 0.012;
    for (const wheel of wheels) wheel.rotation.x = time * 0.5;
  });
  return part.group;
}

export function addRails(
  ctx: SceneBuilder,
  x: number,
  z: number,
  groundY: number,
  length: number,
) {
  for (let step = 0; step <= length; step += 0.5) {
    ctx.voxel.detail(
      x - 0.6,
      groundY,
      z + step,
      1.2,
      0.12,
      0.28,
      PAL.planksDark,
    );
  }
  for (const dx of [-0.28, 0.28]) {
    ctx.voxel.detail(
      x + dx - 0.06,
      groundY + 0.12,
      z,
      0.12,
      0.1,
      length + 0.6,
      PAL.metal,
    );
  }
}

export function pulseLight(
  ctx: SceneBuilder,
  light: PointLight,
  glowMaterial: MeshBasicMaterial | SpriteMaterial,
  base: number,
  amp: number,
  speed: number,
  phase: number,
) {
  ctx.movers.push((time) => {
    const pulse = Math.sin(time * speed + phase) * 0.5 + 0.5;
    light.intensity = base + pulse * amp;
    if ("opacity" in glowMaterial) {
      glowMaterial.opacity = 0.3 + pulse * 0.32;
    }
  });
}

export function swayGroup(
  ctx: SceneBuilder,
  group: Object3D,
  phase: number,
  amp = 0.02,
) {
  ctx.movers.push((time) => {
    group.rotation.z = Math.sin(time * 0.75 + phase) * amp;
    group.rotation.x = Math.sin(time * 0.52 + phase * 1.3) * amp * 0.7;
  });
}

export function drift(
  ctx: SceneBuilder,
  object: Object3D,
  base: Vector3,
  phase: number,
  amp = 0.03,
) {
  ctx.movers.push((time) => {
    object.position.set(
      base.x + Math.sin(time * 0.35 + phase) * amp * 2,
      base.y + Math.sin(time * 0.55 + phase) * amp,
      base.z + Math.cos(time * 0.3 + phase) * amp * 2,
    );
  });
}
