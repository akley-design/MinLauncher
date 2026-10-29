import {
  BoxGeometry,
  BufferGeometry,
  Color,
  ConeGeometry,
  CylinderGeometry,
  DataTexture,
  DoubleSide,
  Float32BufferAttribute,
  Group,
  InstancedMesh,
  LinearFilter,
  LinearMipmapLinearFilter,
  Matrix4,
  Mesh,
  MeshBasicMaterial,
  MeshLambertMaterial,
  NearestFilter,
  NearestMipmapLinearFilter,
  PlaneGeometry,
  PointLight,
  RepeatWrapping,
  RingGeometry,
  SRGBColorSpace,
  SphereGeometry,
  UnsignedByteType,
  RGBAFormat,
} from "three";
import type { DioramaVariant } from "./types";

type VoxelMaterial =
  | "grass"
  | "soil"
  | "stone"
  | "wood"
  | "plaster"
  | "terracotta"
  | "foliage"
  | "obsidian"
  | "portal";

type Block = {
  x: number;
  y: number;
  z: number;
  sx: number;
  sy: number;
  sz: number;
  color: string;
  material: VoxelMaterial;
};

type Particle = {
  mesh: Mesh;
  material: MeshBasicMaterial;
};

export interface World {
  group: Group;
  animate: (time: number, reduced: boolean) => void;
  dispose: () => void;
}

const MATERIALS: VoxelMaterial[] = [
  "grass",
  "soil",
  "stone",
  "wood",
  "plaster",
  "terracotta",
  "foliage",
  "obsidian",
  "portal",
];

const MATERIAL_COLORS: Record<VoxelMaterial, ReadonlySet<string>> = {
  grass: new Set(["#76975b", "#587c4a", "#91ada4", "#758450"]),
  soil: new Set(["#796449", "#b7a782", "#b4a487", "#aea48d", "#aaa38b"]),
  stone: new Set([
    "#686458",
    "#4f514b",
    "#7b7767",
    "#51594e",
    "#5d655e",
    "#414c49",
    "#647366",
    "#47574f",
    "#637163",
    "#6a6b5a",
    "#85816c",
    "#b6ac8b",
    "#6e7264",
  ]),
  wood: new Set(["#544135", "#9f815d", "#614d39", "#635039", "#6d523c"]),
  plaster: new Set(["#ddcca2", "#e6c38a"]),
  terracotta: new Set(["#a05e43", "#bd7952"]),
  foliage: new Set(["#b6c77d", "#63804b", "#456643", "#66894e"]),
  obsidian: new Set(["#211b2a", "#30243a", "#3b2c49", "#18141f"]),
  portal: new Set(["#7e39d9", "#a35cff", "#c27aff"]),
};

function classifyMaterial(color: string): VoxelMaterial {
  const normalized = color.toLowerCase();
  return (
    MATERIALS.find((kind) => MATERIAL_COLORS[kind].has(normalized)) ?? "stone"
  );
}

function pixelNoise(x: number, y: number, seed: number) {
  let value = (x * 374761393 + y * 668265263 + seed * 1442695041) | 0;
  value = Math.imul(value ^ (value >>> 13), 1274126177);
  return ((value ^ (value >>> 16)) >>> 0) / 0xffffffff;
}

function createMaterialTexture(
  kind: VoxelMaterial,
  seed: number,
  anisotropy = 1,
) {
  const size = 16;
  const pixels = new Uint8Array(size * size * 4);
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const coarse = pixelNoise(Math.floor(x / 4), Math.floor(y / 4), seed);
      const fine = pixelNoise(x, y, seed + 11);
      let shade = 222 + Math.round(coarse * 20);
      if (kind === "grass") {
        shade = 214 + Math.round(coarse * 28);
        if (fine > 0.94) shade -= 26;
      } else if (kind === "soil") {
        shade = 198 + Math.round(coarse * 30);
        if (fine > 0.9) shade -= 24;
      } else if (kind === "stone") {
        shade = 200 + Math.round(coarse * 34);
        if (coarse < 0.12) shade -= 18;
        if (fine > 0.98) shade += 12;
      } else if (kind === "wood") {
        shade = 206 + Math.round(coarse * 22);
        if ((x + seed) % 7 === 0) shade -= 17;
        if (fine > 0.98) shade += 10;
      } else if (kind === "plaster") {
        shade = 230 + Math.round(coarse * 18);
        if (fine > 0.98) shade -= 25;
      } else if (kind === "terracotta") {
        shade = 210 + Math.round(coarse * 25);
        if (y % 6 === 0) shade -= 16;
      } else if (kind === "obsidian") {
        shade = 178 + Math.round(coarse * 48);
        if (fine > 0.94) shade += 18;
        if (coarse < 0.16) shade -= 20;
      } else if (kind === "portal") {
        shade = 205 + Math.round(coarse * 42);
        if ((x + y + seed) % 7 === 0) shade += 9;
      } else {
        shade = 208 + Math.round(coarse * 32);
        if (fine > 0.92) shade -= 22;
      }
      const offset = (y * size + x) * 4;
      const channel = Math.max(128, Math.min(255, shade));
      pixels[offset] = channel;
      pixels[offset + 1] = channel;
      pixels[offset + 2] = channel;
      pixels[offset + 3] = 255;
    }
  }
  const texture = new DataTexture(
    pixels,
    size,
    size,
    RGBAFormat,
    UnsignedByteType,
  );
  texture.name = `minlauncher-voxel-${kind}`;
  texture.colorSpace = SRGBColorSpace;
  texture.magFilter = NearestFilter;
  texture.minFilter = NearestMipmapLinearFilter;
  texture.generateMipmaps = true;
  texture.anisotropy = Math.min(4, anisotropy);
  texture.wrapS = texture.wrapT = RepeatWrapping;
  texture.needsUpdate = true;
  return texture;
}

function createVoxelGeometry() {
  const geometry = new BoxGeometry(1, 1, 1);
  const normals = geometry.getAttribute("normal");
  const colors: number[] = [];
  for (let vertex = 0; vertex < normals.count; vertex++) {
    const x = normals.getX(vertex);
    const y = normals.getY(vertex);
    const z = normals.getZ(vertex);
    const shade =
      y > 0.5 ? 1.12 : y < -0.5 ? 0.72 : z > 0.5 ? 0.88 : x > 0.5 ? 0.76 : 0.82;
    colors.push(shade, shade, shade);
  }
  geometry.setAttribute("color", new Float32BufferAttribute(colors, 3));
  return geometry;
}

function addVoxelSurface(
  group: Group,
  blocks: Block[],
  resources: Array<{ dispose: () => void }>,
  anisotropy: number,
) {
  const geometry = createVoxelGeometry();
  resources.push(geometry);
  const matrix = new Matrix4();
  for (const kind of MATERIALS) {
    const instances = blocks.filter((item) => item.material === kind);
    if (instances.length === 0) continue;
    const texture = createMaterialTexture(
      kind,
      MATERIALS.indexOf(kind) + 1,
      anisotropy,
    );
    const material = new MeshLambertMaterial({
      map: texture,
      vertexColors: true,
    });
    resources.push(texture, material);
    const mesh = new InstancedMesh(geometry, material, instances.length);
    instances.forEach((item, index) => {
      matrix
        .makeScale(item.sx, item.sy, item.sz)
        .setPosition(item.x, item.y, item.z);
      mesh.setMatrixAt(index, matrix);
      mesh.setColorAt(index, new Color(item.color));
    });
    mesh.castShadow = true;
    mesh.receiveShadow = true;
    mesh.computeBoundingSphere();
    group.add(mesh);
    resources.push({ dispose: () => mesh.dispose() });
  }
}

export function buildWorld(variant: DioramaVariant, anisotropy = 1): World {
  const group = new Group();
  const blocks: Block[] = [];
  const resources: Array<{ dispose: () => void }> = [];
  const movers: Array<(time: number) => void> = [];
  const particles: Particle[] = [];
  const block = (
    x: number,
    y: number,
    z: number,
    sx: number,
    sy: number,
    sz: number,
    color: string,
  ) => {
    blocks.push({
      x,
      y,
      z,
      sx,
      sy,
      sz,
      color,
      material: classifyMaterial(color),
    });
  };
  const mesh = (
    shape: BufferGeometry,
    color: string,
    emissive = "#000000",
    opacity = 1,
  ) => {
    const surface = new MeshLambertMaterial({
      color,
      emissive,
      emissiveIntensity: 1.1,
      transparent: opacity < 1,
      opacity,
      depthWrite: opacity === 1,
    });
    const object = new Mesh(shape, surface);
    object.castShadow = opacity === 1;
    object.receiveShadow = true;
    resources.push(shape, surface);
    group.add(object);
    return object;
  };
  const glow = (color: string, intensity: number, distance: number) => {
    const light = new PointLight(color, intensity, distance, 2);
    group.add(light);
    return light;
  };
  const noise = (x: number, z: number) =>
    (Math.sin(x * 17.13 + z * 41.77) * 43758.54) % 1;

  if (variant === "island") {
    // Chunked earth strata keep a clean voxel silhouette around the shoreline.
    for (let x = -5; x <= 5; x++)
      for (let z = -4; z <= 4; z++) {
        const edge = (x / 5.4) ** 2 + (z / 4.5) ** 2;
        if (edge > 1.07 + noise(x, z) * 0.12) continue;
        const px = x * 0.58;
        const pz = z * 0.58;
        const river = x === 2 || (x === 3 && z < 1);
        const top = river ? -0.1 : 0.05 + Math.max(0, noise(x, z)) * 0.12;
        const depth = 0.6 + (1 - Math.min(1, edge)) * 1.25;
        const edgeTint = noise(x + 9, z + 3) > 0.5 ? "#686458" : "#4f514b";
        block(px, -depth / 2 - 0.3, pz, 0.57, depth, 0.57, edgeTint);
        block(px, -0.12, pz, 0.57, 0.42, 0.57, river ? "#b7a782" : "#796449");
        block(
          px,
          top,
          pz,
          0.535,
          0.17,
          0.535,
          river ? "#91ada4" : noise(x, z) > 0.1 ? "#76975b" : "#587c4a",
        );
        if (!river && noise(x + 13, z) > 0.42 && x > -2) {
          block(px + 0.1, top + 0.2, pz, 0.05, 0.25, 0.05, "#b6c77d");
          if (z > 0)
            block(px + 0.1, top + 0.33, pz, 0.13, 0.09, 0.13, "#e6c38a");
        }
      }

    // Raised timber-and-plaster cottage with a legible stepped roof and trim.
    block(-0.95, 0.25, -0.25, 2.1, 0.32, 1.8, "#aea48d");
    block(-0.95, 1.06, -0.25, 1.85, 1.4, 1.55, "#ddcca2");
    for (const x of [-1.9, -0.02])
      for (const z of [-1.04, 0.55])
        block(x, 1.08, z, 0.14, 1.65, 0.14, "#544135");
    block(-0.95, 0.63, 0.57, 1.85, 0.1, 0.1, "#9f815d");
    block(-1.4, 0.93, 0.58, 0.4, 1.08, 0.08, "#614d39");
    block(-1.42, 0.85, 0.64, 0.04, 0.04, 0.04, "#e3b96e");
    const windowMeshes: Mesh[] = [];
    for (const x of [-0.6, -1.5]) {
      const window = mesh(
        new BoxGeometry(0.48, 0.52, 0.05),
        "#ffe3a0",
        "#d88f38",
      );
      window.position.set(x, 1.2, 0.565);
      windowMeshes.push(window);
      block(x, 1.2, 0.61, 0.045, 0.56, 0.04, "#635039");
      block(x, 1.2, 0.61, 0.52, 0.045, 0.04, "#635039");
    }
    for (let i = 0; i < 5; i++) {
      const width = 2.35 - i * 0.39;
      block(
        -0.95,
        1.87 + i * 0.18,
        -0.25,
        width,
        0.2,
        2.04 - i * 0.14,
        i % 2 ? "#a05e43" : "#bd7952",
      );
    }
    block(-0.95, 2.75, -0.25, 0.24, 0.18, 0.24, "#a05e43");
    block(-1.42, 2.46, -0.73, 0.35, 0.82, 0.36, "#6e7264");
    const warmLight = glow("#ffbd69", 1.3, 3.8);
    warmLight.position.set(-0.9, 1.1, 0.8);
    movers.push((time) => {
      const breath = 0.5 + 0.5 * Math.sin(time * 1.1);
      windowMeshes.forEach((window, index) => {
        (window.material as MeshLambertMaterial).emissiveIntensity =
          0.8 + breath * 0.32 + index * 0.03;
      });
      warmLight.intensity = 1.15 + breath * 0.35;
    });

    // Soft, expanding puffs replace the old rigid cubes above the chimney.
    for (let i = 0; i < 5; i++) {
      const material = new MeshLambertMaterial({
        color: "#c7cabb",
        transparent: true,
        opacity: 0.18,
        depthWrite: false,
      });
      const puff = new Mesh(new SphereGeometry(0.13, 7, 6), material);
      puff.position.set(-1.42, 2.9, -0.73);
      resources.push(puff.geometry, material);
      group.add(puff);
      const phase = i / 5;
      movers.push((time) => {
        const life = (time * 0.12 + phase) % 1;
        const swell = 0.65 + life * 1.4;
        puff.position.set(
          -1.42 + Math.sin(time * 0.7 + i * 1.7) * (0.08 + life * 0.12),
          2.9 + life * 1.3,
          -0.73 + Math.cos(time * 0.6 + i) * 0.07,
        );
        puff.scale.set(swell, swell * 0.9, swell);
        material.opacity = (1 - life) * 0.24;
      });
    }

    // Offset canopy layers sway gently from the trunk instead of wobbling as one block.
    block(2.15, 0.75, -1.25, 0.23, 1.5, 0.23, "#635039");
    for (let i = 0; i < 3; i++) {
      const leaves = mesh(
        new BoxGeometry(1.45 - i * 0.35, 0.6, 1.35 - i * 0.32),
        i % 2 ? "#63804b" : "#456643",
      );
      leaves.position.set(2.15, 1.45 + i * 0.48, -1.25);
      movers.push((time) => {
        leaves.rotation.z = Math.sin(time * 0.62 + i * 0.7) * 0.035;
        leaves.rotation.x = Math.sin(time * 0.47 + i) * 0.018;
      });
    }
    block(-2.15, 0.55, -1.45, 0.16, 0.95, 0.16, "#6d523c");
    block(-2.15, 1.18, -1.45, 0.92, 0.8, 0.86, "#66894e");

    const waterTexture = createMaterialTexture("grass", 91);
    waterTexture.name = "minlauncher-river-flow";
    waterTexture.magFilter = LinearFilter;
    waterTexture.minFilter = LinearMipmapLinearFilter;
    waterTexture.repeat.set(1, 1.8);
    const waterMaterial = new MeshBasicMaterial({
      color: "#84d6ca",
      map: waterTexture,
      transparent: true,
      opacity: 0.74,
      depthWrite: false,
    });
    const water = new Mesh(new PlaneGeometry(0.94, 4.3), waterMaterial);
    water.rotation.x = -Math.PI / 2;
    water.position.set(1.28, 0.14, 0.05);
    group.add(water);
    resources.push(water.geometry, waterMaterial, waterTexture);
    movers.push((time) => {
      waterMaterial.map!.offset.y = (time * 0.07) % 1;
      water.position.y = 0.14 + Math.sin(time * 0.9) * 0.018;
    });

    const fallTexture = createMaterialTexture("grass", 107);
    fallTexture.name = "minlauncher-waterfall-flow";
    fallTexture.magFilter = LinearFilter;
    fallTexture.minFilter = LinearMipmapLinearFilter;
    fallTexture.repeat.set(1.4, 1.8);
    const fallMaterial = new MeshBasicMaterial({
      color: "#92e1d5",
      map: fallTexture,
      transparent: true,
      opacity: 0.72,
      depthWrite: false,
    });
    const fall = new Mesh(new PlaneGeometry(0.87, 1.9), fallMaterial);
    fall.position.set(1.28, -0.82, 2.2);
    group.add(fall);
    resources.push(fall.geometry, fallMaterial, fallTexture);
    movers.push((time) => {
      fallMaterial.map!.offset.y = (time * 0.28) % 1;
    });

    for (let i = 0; i < 5; i++) {
      const rippleMaterial = new MeshBasicMaterial({
        color: "#d7fff1",
        transparent: true,
        opacity: 0.55,
        depthWrite: false,
        side: DoubleSide,
      });
      const ripple = new Mesh(new RingGeometry(0.07, 0.1, 16), rippleMaterial);
      ripple.rotation.x = -Math.PI / 2;
      ripple.position.set(1.28 + Math.sin(i) * 0.17, 0.172, -1.45 + i * 0.72);
      group.add(ripple);
      resources.push(ripple.geometry, rippleMaterial);
      const phase = i / 5;
      movers.push((time) => {
        const life = (time * 0.22 + phase) % 1;
        ripple.scale.setScalar(0.65 + life * 2.2);
        rippleMaterial.opacity = (1 - life) * 0.44;
        ripple.position.z = -1.5 + i * 0.72 + Math.sin(time * 0.5 + i) * 0.035;
      });
    }
    for (let i = 0; i < 7; i++)
      block(
        -1.4 + i * 0.22,
        0.18,
        1.05 + i * 0.12,
        0.32,
        0.12,
        0.32,
        "#b4a487",
      );
  } else if (variant === "cave") {
    // A compact mine entrance with a clear tunnel mouth and a readable rail line.
    for (let x = -5; x <= 5; x++) {
      for (let z = -3; z <= 3; z++) {
        if ((x / 5.4) ** 2 + (z / 3.6) ** 2 > 1.08) continue;
        const px = x * 0.48;
        const pz = z * 0.48;
        const edge = Math.abs(x) > 3 || z === -3;
        block(
          px,
          -0.36,
          pz,
          0.48,
          0.34,
          0.48,
          edge ? (noise(x, z) > 0 ? "#686458" : "#4f514b") : "#5d655e",
        );
        if (edge && Math.abs(x) > 3)
          block(
            px,
            -0.02 + (Math.abs(noise(x, z)) % 0.2),
            pz,
            0.48,
            0.42,
            0.48,
            noise(x + 3, z) > 0 ? "#647366" : "#47574f",
          );
      }
    }
    // Set-back dark end wall, framed by irregular stone shoulders.
    for (let x = -2; x <= 2; x++)
      for (let y = 0; y < 4; y++)
        block(
          x * 0.48,
          y * 0.43 + 0.02,
          -1.42,
          0.47,
          0.43,
          0.16,
          (x + y) % 5 === 0 ? "#30243a" : "#211b2a",
        );
    for (const side of [-1, 1]) {
      for (let row = 0; row < 4; row++) {
        const y = 0.14 + row * 0.43;
        block(
          side * 1.18,
          y,
          -1.12,
          0.48,
          0.45,
          0.56,
          row === 0 ? "#7b7767" : row % 2 ? "#686458" : "#5d655e",
        );
      }
      block(side * 1.02, 1.88, -1.12, 0.7, 0.42, 0.56, "#686458");
      // Timber braces sit on the entrance face and leave the opening unobstructed.
      block(side * 0.84, 0.54, -0.66, 0.12, 1.34, 0.12, "#544135");
    }
    block(0, 1.34, -0.66, 1.82, 0.12, 0.12, "#9f815d");
    block(0, 1.92, -1.12, 1.25, 0.42, 0.56, "#7b7767");

    // Rails run from the foreground through the opening to the dark end wall.
    for (const x of [-0.29, 0.29])
      block(x, -0.08, -0.1, 0.075, 0.09, 2.65, "#9f815d");
    for (let i = 0; i < 10; i++)
      block(0, -0.12, 1.02 - i * 0.29, 0.76, 0.075, 0.11, "#614d39");

    // Ore crystals cluster against the shoulders, outside the cart's path.
    const ore = mesh(new ConeGeometry(0.17, 0.68, 5), "#8ed9c7", "#286c5d");
    ore.position.set(-1.48, 0.28, 0.5);
    ore.rotation.z = -0.12;
    const oreSmall = mesh(
      new ConeGeometry(0.12, 0.47, 5),
      "#b7f4dc",
      "#347e69",
    );
    oreSmall.position.set(-1.68, 0.2, 0.37);
    oreSmall.rotation.z = 0.18;
    const goldVein = mesh(
      new BoxGeometry(0.12, 0.075, 0.035),
      "#e6c38a",
      "#9d642b",
    );
    goldVein.position.set(1.51, 0.58, 0.5);
    const crystals = [ore, oreSmall];
    movers.push((time) =>
      crystals.forEach((crystal, i) => {
        crystal.rotation.y = Math.sin(time * 0.6 + i) * 0.08;
        (crystal.material as MeshLambertMaterial).emissiveIntensity =
          0.8 + Math.sin(time * 1.1 + i) * 0.25;
      }),
    );

    // A low mine cart gives the rails scale without hiding the tunnel.
    const cart = mesh(new BoxGeometry(0.48, 0.23, 0.42), "#9f815d");
    cart.position.set(0, 0.08, 0.74);
    for (const x of [-0.18, 0.18]) {
      const wheel = mesh(new CylinderGeometry(0.09, 0.09, 0.07, 8), "#414c49");
      wheel.rotation.z = Math.PI / 2;
      wheel.position.set(x, -0.055, 0.74);
    }
    const caveLight = glow("#8ddac1", 4.2, 5.5);
    caveLight.position.set(0, 1.3, -0.48);
    movers.push((time) => {
      caveLight.intensity = 3.8 + Math.sin(time * 0.55) * 0.45;
      cart.position.z = 0.72 + Math.sin(time * 0.28) * 0.08;
    });
  } else {
    // A fresh floating altar supports a narrow, unmistakable obsidian gateway.
    for (let x = -5; x <= 5; x++) {
      for (let z = -3; z <= 3; z++) {
        if ((x / 5.2) ** 2 + (z / 3.35) ** 2 > 1.08) continue;
        const px = x * 0.48;
        const pz = z * 0.48;
        const edge = (x / 5.2) ** 2 + (z / 3.35) ** 2 > 0.72;
        block(
          px,
          -0.22,
          pz,
          0.47,
          0.28,
          0.47,
          edge ? "#796449" : noise(x, z) > 0 ? "#76975b" : "#587c4a",
        );
        block(
          px,
          -0.49,
          pz,
          0.45,
          0.26,
          0.45,
          (x + z) % 3 ? "#30243a" : "#3b2c49",
        );
        if (edge && (x + z) % 2 === 0)
          block(px, -0.73, pz, 0.42, 0.22, 0.42, "#211b2a");
      }
    }
    // Broad, aligned steps make a clean approach from the front of the island.
    for (let step = 0; step < 4; step++)
      block(
        0,
        -0.015 + step * 0.045,
        1.12 + step * 0.38,
        0.94 - step * 0.08,
        0.16,
        0.34,
        step % 2 ? "#b7a782" : "#85816c",
      );

    // Slender segmented pillars and a stepped lintel keep the purple opening dominant.
    for (const side of [-1, 1]) {
      for (let row = 0; row < 6; row++) {
        const y = 0.16 + row * 0.48;
        block(
          side * 1.08,
          y,
          -0.18,
          0.42,
          0.47,
          0.46,
          row % 2 ? "#30243a" : "#211b2a",
        );
        block(
          side * 0.84,
          y,
          0.065,
          0.08,
          0.32,
          0.035,
          row % 2 ? "#3b2c49" : "#30243a",
        );
      }
      block(side * 1.08, 0.12, -0.18, 0.58, 0.22, 0.58, "#3b2c49");
    }
    for (let i = 0; i < 5; i++)
      block(
        (i - 2) * 0.52,
        3.02 + (Math.abs(i - 2) === 2 ? -0.12 : 0),
        -0.18,
        0.5,
        0.48,
        0.46,
        i === 2 ? "#3b2c49" : "#211b2a",
      );

    const portalGeometry = new PlaneGeometry(1.64, 2.42, 12, 20);
    const portalMaterial = new MeshBasicMaterial({
      color: "#8f43e8",
      transparent: true,
      opacity: 0.94,
      side: DoubleSide,
      depthWrite: false,
    });
    const portal = new Mesh(portalGeometry, portalMaterial);
    portal.name = "Portal_Energy_Surface";
    portal.position.set(0, 1.72, 0.015);
    group.add(portal);
    resources.push(portalGeometry, portalMaterial);
    const positions = portalGeometry.getAttribute("position");
    const normals = portalGeometry.getAttribute("normal");
    const baseZ = Float32Array.from({ length: positions.count }, (_, i) =>
      positions.getZ(i),
    );
    movers.push((time) => {
      for (let i = 0; i < positions.count; i++) {
        const x = positions.getX(i),
          y = positions.getY(i);
        const phase = y * 3.6 + x * 2.1 + time * 1.5;
        const ripple = Math.sin(phase) * 0.055;
        positions.setZ(i, baseZ[i]! + ripple);
        const dx = -Math.cos(phase) * 0.115,
          dy = -Math.cos(phase) * 0.2;
        const length = Math.sqrt(dx * dx + dy * dy + 1);
        normals.setXYZ(i, dx / length, dy / length, 1 / length);
      }
      positions.needsUpdate = true;
      normals.needsUpdate = true;
      portalMaterial.opacity = 0.84 + Math.sin(time * 0.8) * 0.08;
    });

    // Four restrained light streaks drift through the energy field.
    for (let i = 0; i < 4; i++) {
      const streakMaterial = new MeshBasicMaterial({
        color: i % 2 ? "#e5caff" : "#bd7aff",
        transparent: true,
        opacity: 0.75,
        depthWrite: false,
      });
      const streak = new Mesh(
        new PlaneGeometry(0.42 + (i % 2) * 0.24, 0.035),
        streakMaterial,
      );
      streak.position.set(i % 2 ? 0.28 : -0.24, 0.82 + i * 0.48, 0.055);
      group.add(streak);
      resources.push(streak.geometry, streakMaterial);
      movers.push((time) => {
        streak.position.x =
          (i % 2 ? 0.28 : -0.24) + Math.sin(time * 0.65 + i) * 0.2;
        streakMaterial.opacity = 0.5 + Math.sin(time * 1.2 + i) * 0.22;
      });
    }
    // Obsidian chips and moss are embedded along the platform's rim.
    for (let i = 0; i < 8; i++) {
      const x = -2.15 + i * 0.61;
      if (Math.abs(x) < 1.2) continue;
      block(
        x,
        0.015,
        -1.35 + (i % 3) * 0.22,
        0.18,
        0.14,
        0.17,
        i % 2 ? "#76975b" : "#3b2c49",
      );
    }
    const portalLight = glow("#ad63ff", 5.5, 5.2);
    portalLight.position.set(0, 1.7, 0.72);
    movers.push((time) => {
      portalLight.intensity = 4.8 + Math.sin(time * 0.75) * 0.6;
    });
  }
  addVoxelSurface(group, blocks, resources, anisotropy);

  // Small faceted motes follow a distinct orbit for each scene; their count is
  // fixed so animation frames allocate nothing and stay within the frame budget.
  const particleCount =
    variant === "island" ? 10 : variant === "cave" ? 14 : 16;
  const particleGeometry = new SphereGeometry(0.035, 5, 4);
  resources.push(particleGeometry);
  for (let i = 0; i < particleCount; i++) {
    const material = new MeshBasicMaterial({
      color:
        variant === "cave"
          ? "#c2f4dc"
          : variant === "portal"
            ? "#d5b1ff"
            : "#f2dfa3",
      transparent: true,
      opacity: 0.8,
      depthWrite: false,
    });
    const mote = new Mesh(particleGeometry, material);
    group.add(mote);
    particles.push({ mesh: mote, material });
    resources.push(material);
  }

  return {
    group,
    animate(time, reduced) {
      if (reduced) {
        particles.forEach(({ mesh: mote }) => {
          mote.visible = false;
        });
        return;
      }
      const float =
        Math.sin(time * 0.72) * (variant === "island" ? 0.065 : 0.025);
      group.position.y = float;
      group.rotation.y =
        Math.sin(time * 0.24) * (variant === "island" ? 0.035 : 0.018);
      movers.forEach((move) => move(time));
      particles.forEach(({ mesh: mote, material }, i) => {
        const phase = time * (variant === "portal" ? 0.72 : 0.45) + i * 2.399;
        const pulse =
          0.45 + 0.55 * (0.5 + 0.5 * Math.sin(time * 1.7 + i * 1.3));
        if (variant === "portal") {
          const radius = 1.1 + (i % 4) * 0.18;
          mote.position.set(
            Math.cos(phase) * radius,
            0.4 + (i % 6) * 0.4,
            -0.1 + Math.sin(phase) * 0.1,
          );
        } else if (variant === "cave") {
          mote.position.set(
            Math.cos(phase) * (1.6 + (i % 3) * 0.35),
            0.15 + (i % 5) * 0.38 + Math.sin(phase * 0.7) * 0.12,
            Math.sin(phase) * 1.15,
          );
        } else {
          mote.position.set(
            Math.cos(phase) * (1.8 + (i % 3) * 0.35),
            0.25 + (i % 5) * 0.33 + Math.sin(phase * 0.8) * 0.16,
            Math.sin(phase) * 1.8,
          );
        }
        const scale = 0.65 + pulse * 0.7;
        mote.scale.setScalar(scale);
        material.opacity = 0.26 + pulse * 0.64;
        mote.visible = true;
      });
    },
    dispose() {
      resources.forEach((resource) => resource.dispose());
    },
  };
}
