import {
  BufferGeometry,
  Color,
  Float32BufferAttribute,
  Group,
  Mesh,
  MeshBasicMaterial,
  MeshStandardMaterial,
} from "three";
import type { ResourceRegistry } from "./resources";
import type { SiteTile, TextureBank } from "./textures";
import { hash3 } from "./motion";

export const VOXEL_SIZE = 0.34;

export interface BlockStyle {
  top: SiteTile;
  side: SiteTile;
  bottom: SiteTile;
  tint: string;
  topTint?: string;
  sideTint?: string;
  bottomTint?: string;
  emissive?: boolean;
}

export interface BlockData {
  x: number;
  y: number;
  z: number;
  style: BlockStyle;
  tint?: string;
}

export interface DetailData {
  x: number;
  y: number;
  z: number;
  w: number;
  h: number;
  d: number;
  style: BlockStyle;
  tint?: string;
}

interface FaceDefinition {
  nx: number;
  ny: number;
  nz: number;
  shade: number;
  kind: "top" | "side" | "bottom";
  corners: ReadonlyArray<readonly [number, number, number]>;
}

const FACES: FaceDefinition[] = [
  {
    nx: 1,
    ny: 0,
    nz: 0,
    shade: 0.76,
    kind: "side",
    corners: [
      [1, 0, 1],
      [1, 0, 0],
      [1, 1, 0],
      [1, 1, 1],
    ],
  },
  {
    nx: -1,
    ny: 0,
    nz: 0,
    shade: 0.7,
    kind: "side",
    corners: [
      [0, 0, 0],
      [0, 0, 1],
      [0, 1, 1],
      [0, 1, 0],
    ],
  },
  {
    nx: 0,
    ny: 1,
    nz: 0,
    shade: 1,
    kind: "top",
    corners: [
      [0, 1, 1],
      [1, 1, 1],
      [1, 1, 0],
      [0, 1, 0],
    ],
  },
  {
    nx: 0,
    ny: -1,
    nz: 0,
    shade: 0.56,
    kind: "bottom",
    corners: [
      [0, 0, 0],
      [1, 0, 0],
      [1, 0, 1],
      [0, 0, 1],
    ],
  },
  {
    nx: 0,
    ny: 0,
    nz: 1,
    shade: 0.88,
    kind: "side",
    corners: [
      [0, 0, 1],
      [1, 0, 1],
      [1, 1, 1],
      [0, 1, 1],
    ],
  },
  {
    nx: 0,
    ny: 0,
    nz: -1,
    shade: 0.8,
    kind: "side",
    corners: [
      [1, 0, 0],
      [0, 0, 0],
      [0, 1, 0],
      [1, 1, 0],
    ],
  },
];

const FACE_UVS: ReadonlyArray<readonly [number, number]> = [
  [0, 0],
  [1, 0],
  [1, 1],
  [0, 1],
];

const AO_LEVELS = [0.58, 0.72, 0.86, 1];

const tintCache = new Map<string, Color>();

function tintColor(hex: string) {
  const cached = tintCache.get(hex);
  if (cached) return cached;
  const color = new Color(hex);
  tintCache.set(hex, color);
  return color;
}

function key(x: number, y: number, z: number) {
  return `${x},${y},${z}`;
}

export class VoxelBuilder {
  private blocks = new Map<string, BlockData>();
  private details: DetailData[] = [];

  set(x: number, y: number, z: number, style: BlockStyle, tint?: string) {
    this.blocks.set(key(x, y, z), { x, y, z, style, tint });
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
    for (let ix = 0; ix < width; ix++)
      for (let iy = 0; iy < height; iy++)
        for (let iz = 0; iz < depth; iz++)
          this.set(x + ix, y + iy, z + iz, style, tint);
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
    this.details.push({
      x,
      y,
      z,
      w: width,
      h: height,
      d: depth,
      style,
      tint,
    });
    return this;
  }

  has(x: number, y: number, z: number) {
    return this.blocks.has(key(x, y, z));
  }

  blockCount() {
    return this.blocks.size;
  }

  blocksInOrder() {
    return this.blocks.values();
  }

  detailList() {
    return this.details;
  }
}

interface FaceGroup {
  tile: SiteTile;
  emissive: boolean;
  positions: number[];
  normals: number[];
  uvs: number[];
  colors: number[];
  indices: number[];
}

export interface VoxelBuildResult {
  group: Group;
  triangles: number;
  quads: number;
  meshes: number;
}

function faceTint(block: BlockData, kind: FaceDefinition["kind"]) {
  const override = block.tint;
  const base = override ?? block.style.tint;
  if (kind === "top") return block.style.topTint ?? base;
  if (kind === "bottom") return block.style.bottomTint ?? base;
  return block.style.sideTint ?? base;
}

function faceTile(block: BlockData, kind: FaceDefinition["kind"]) {
  if (kind === "top") return block.style.top;
  if (kind === "bottom") return block.style.bottom;
  return block.style.side;
}

function cornerOcclusion(
  builder: VoxelBuilder,
  face: FaceDefinition,
  x: number,
  y: number,
  z: number,
  cx: number,
  cy: number,
  cz: number,
) {
  let side1: number;
  let side2: number;
  let corner: number;
  if (face.nx !== 0) {
    const dy = cy === 1 ? 1 : -1;
    const dz = cz === 1 ? 1 : -1;
    side1 = builder.has(x + face.nx, y + dy, z) ? 1 : 0;
    side2 = builder.has(x + face.nx, y, z + dz) ? 1 : 0;
    corner = builder.has(x + face.nx, y + dy, z + dz) ? 1 : 0;
  } else if (face.ny !== 0) {
    const dx = cx === 1 ? 1 : -1;
    const dz = cz === 1 ? 1 : -1;
    side1 = builder.has(x + dx, y + face.ny, z) ? 1 : 0;
    side2 = builder.has(x, y + face.ny, z + dz) ? 1 : 0;
    corner = builder.has(x + dx, y + face.ny, z + dz) ? 1 : 0;
  } else {
    const dx = cx === 1 ? 1 : -1;
    const dy = cy === 1 ? 1 : -1;
    side1 = builder.has(x + dx, y, z + face.nz) ? 1 : 0;
    side2 = builder.has(x, y + dy, z + face.nz) ? 1 : 0;
    corner = builder.has(x + dx, y + dy, z + face.nz) ? 1 : 0;
  }
  if (side1 && side2) return AO_LEVELS[0]!;
  return AO_LEVELS[3 - (side1 + side2 + corner)]!;
}

function pushFace(
  group: FaceGroup,
  positions: ReadonlyArray<number>,
  normal: readonly [number, number, number],
  colors: ReadonlyArray<number>,
) {
  const base = group.positions.length / 3;
  for (let index = 0; index < 4; index++) {
    group.positions.push(positions[index * 3]!);
    group.positions.push(positions[index * 3 + 1]!);
    group.positions.push(positions[index * 3 + 2]!);
    group.normals.push(normal[0], normal[1], normal[2]);
    group.uvs.push(FACE_UVS[index]![0], FACE_UVS[index]![1]);
    group.colors.push(colors[index * 3]!);
    group.colors.push(colors[index * 3 + 1]!);
    group.colors.push(colors[index * 3 + 2]!);
  }
  group.indices.push(base, base + 1, base + 2, base, base + 2, base + 3);
}

export function buildVoxelMeshes(
  builder: VoxelBuilder,
  bank: TextureBank,
  resources: ResourceRegistry,
  origin: { x: number; y: number; z: number } = { x: 0, y: 0, z: 0 },
): VoxelBuildResult {
  const groups = new Map<string, FaceGroup>();
  const groupFor = (tile: SiteTile, emissive: boolean) => {
    const id = `${emissive ? "glow" : "lit"}:${tile}`;
    let group = groups.get(id);
    if (!group) {
      group = {
        tile,
        emissive,
        positions: [],
        normals: [],
        uvs: [],
        colors: [],
        indices: [],
      };
      groups.set(id, group);
    }
    return group;
  };

  for (const block of builder.blocksInOrder()) {
    const jitter = 0.94 + hash3(block.x, block.y, block.z) * 0.12;
    for (const face of FACES) {
      if (builder.has(block.x + face.nx, block.y + face.ny, block.z + face.nz))
        continue;
      const tile = faceTile(block, face.kind);
      const color = tintColor(faceTint(block, face.kind));
      const group = groupFor(tile, Boolean(block.style.emissive));
      const positions: number[] = [];
      const colors: number[] = [];
      for (const corner of face.corners) {
        const cx = corner[0];
        const cy = corner[1];
        const cz = corner[2];
        positions.push(
          (block.x + cx) * VOXEL_SIZE - origin.x,
          (block.y + cy) * VOXEL_SIZE - origin.y,
          (block.z + cz) * VOXEL_SIZE - origin.z,
        );
        const factor = block.style.emissive
          ? jitter
          : face.shade *
            jitter *
            cornerOcclusion(
              builder,
              face,
              block.x,
              block.y,
              block.z,
              cx,
              cy,
              cz,
            );
        colors.push(color.r * factor, color.g * factor, color.b * factor);
      }
      pushFace(group, positions, [face.nx, face.ny, face.nz], colors);
    }
  }

  for (const detail of builder.detailList()) {
    for (const face of FACES) {
      const tile = faceTile(
        {
          x: 0,
          y: 0,
          z: 0,
          style: detail.style,
          tint: detail.tint,
        },
        face.kind,
      );
      const color = tintColor(
        faceTint(
          {
            x: 0,
            y: 0,
            z: 0,
            style: detail.style,
            tint: detail.tint,
          },
          face.kind,
        ),
      );
      const group = groupFor(tile, Boolean(detail.style.emissive));
      const positions: number[] = [];
      const colors: number[] = [];
      for (const corner of face.corners) {
        positions.push(
          (detail.x + corner[0] * detail.w) * VOXEL_SIZE - origin.x,
          (detail.y + corner[1] * detail.h) * VOXEL_SIZE - origin.y,
          (detail.z + corner[2] * detail.d) * VOXEL_SIZE - origin.z,
        );
        const factor = detail.style.emissive ? 1 : face.shade;
        colors.push(color.r * factor, color.g * factor, color.b * factor);
      }
      pushFace(group, positions, [face.nx, face.ny, face.nz], colors);
    }
  }

  const group = new Group();
  group.name = "ml-site-voxels";
  let quads = 0;
  let meshes = 0;
  for (const faceGroup of groups.values()) {
    if (faceGroup.indices.length === 0) continue;
    const geometry = new BufferGeometry();
    geometry.setAttribute(
      "position",
      new Float32BufferAttribute(faceGroup.positions, 3),
    );
    geometry.setAttribute(
      "normal",
      new Float32BufferAttribute(faceGroup.normals, 3),
    );
    geometry.setAttribute("uv", new Float32BufferAttribute(faceGroup.uvs, 2));
    geometry.setAttribute(
      "color",
      new Float32BufferAttribute(faceGroup.colors, 3),
    );
    geometry.setIndex(faceGroup.indices);
    geometry.computeBoundingSphere();
    const material = faceGroup.emissive
      ? new MeshBasicMaterial({
          map: bank.get(faceGroup.tile),
          vertexColors: true,
          toneMapped: false,
        })
      : new MeshStandardMaterial({
          map: bank.get(faceGroup.tile),
          vertexColors: true,
          roughness: 0.92,
          metalness: 0,
          dithering: true,
        });
    resources.add(geometry);
    resources.add(material);
    const mesh = new Mesh(geometry, material);
    mesh.name = `ml-site-${faceGroup.emissive ? "glow" : "lit"}-${faceGroup.tile}`;
    mesh.castShadow = !faceGroup.emissive;
    mesh.receiveShadow = !faceGroup.emissive;
    group.add(mesh);
    quads += faceGroup.indices.length / 6;
    meshes++;
  }

  return { group, triangles: quads * 2, quads, meshes };
}
