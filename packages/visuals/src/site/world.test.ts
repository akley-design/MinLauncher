import {
  DataTexture,
  Mesh,
  MeshStandardMaterial,
  NearestFilter,
  NearestMipmapLinearFilter,
  Points,
  ShaderMaterial,
  type Object3D,
} from "three";
import { describe, expect, it } from "vitest";
import type { DioramaVariant } from "../types";
import { PAL } from "./palette";
import { ResourceRegistry } from "./resources";
import { TextureBank } from "./textures";
import { VOXEL_SIZE, VoxelBuilder, buildVoxelMeshes } from "./voxel";
import { buildSiteWorld } from "./world";

function collectMeshes(root: Object3D) {
  const meshes: Mesh[] = [];
  root.traverse((object) => {
    if (object instanceof Mesh) meshes.push(object);
  });
  return meshes;
}

function collectPoints(root: Object3D) {
  const points: Points[] = [];
  root.traverse((object) => {
    if (object instanceof Points) points.push(object);
  });
  return points;
}

function fixture(build: (builder: VoxelBuilder) => void) {
  const bank = new TextureBank();
  const resources = new ResourceRegistry();
  resources.add(bank);
  const builder = new VoxelBuilder();
  build(builder);
  const result = buildVoxelMeshes(builder, bank, resources);
  return { result, resources };
}

function topFaceMinimum(builder: VoxelBuilder) {
  const { result, resources } = (() => {
    const bank = new TextureBank();
    const registry = new ResourceRegistry();
    registry.add(bank);
    const content = buildVoxelMeshes(builder, bank, registry);
    return { result: content, resources: registry };
  })();
  let minimum = Infinity;
  for (const mesh of collectMeshes(result.group)) {
    const positions = mesh.geometry.getAttribute("position");
    const normals = mesh.geometry.getAttribute("normal");
    const colors = mesh.geometry.getAttribute("color");
    for (let index = 0; index < positions.count; index++) {
      if (normals.getY(index) < 0.9) continue;
      if (Math.abs(positions.getY(index) - VOXEL_SIZE) > 1e-6) continue;
      minimum = Math.min(minimum, colors.getX(index));
    }
  }
  resources.dispose();
  return minimum;
}

describe("site voxel dioramas", () => {
  it.each(["island", "cave", "portal"] as DioramaVariant[])(
    "builds a textured, animated %s world with a bounded budget",
    (variant) => {
      const world = buildSiteWorld(variant);
      const meshes = collectMeshes(world.group);
      const lit = meshes.filter(
        (mesh) =>
          mesh.material instanceof MeshStandardMaterial &&
          mesh.material.map instanceof DataTexture,
      );
      expect(lit.length).toBeGreaterThan(0);
      expect(lit.length).toBeLessThanOrEqual(24);
      for (const mesh of lit) {
        const material = mesh.material as MeshStandardMaterial;
        expect(material.vertexColors).toBe(true);
        expect(material.map).toBeInstanceOf(DataTexture);
        expect(material.map?.image.width).toBe(16);
        expect(material.map?.magFilter).toBe(NearestFilter);
        expect(material.map?.minFilter).toBe(NearestMipmapLinearFilter);
        expect(mesh.geometry.getAttribute("color")).toBeDefined();
      }
      expect(world.triangles).toBeGreaterThan(1000);
      expect(world.triangles).toBeLessThanOrEqual(60000);

      world.animate(0, false);
      const initialY = world.group.position.y;
      const initialRotation = world.group.rotation.y;
      world.animate(2, false);
      expect(world.group.position.y).not.toBe(initialY);
      expect(world.group.rotation.y).not.toBe(initialRotation);

      const points = collectPoints(world.group);
      expect(points.length).toBeGreaterThan(0);
      world.animate(2, true);
      expect(points.some((field) => field.visible === false)).toBe(true);

      world.dispose();
      world.dispose();
    },
  );

  it("does not rebuild scene objects while animating", () => {
    const world = buildSiteWorld("island");
    const childCount = world.group.children.length;
    const geometries = new Set(
      collectMeshes(world.group).map((mesh) => mesh.geometry.uuid),
    );
    for (let frame = 0; frame < 120; frame++) world.animate(frame / 60, false);
    expect(world.group.children.length).toBe(childCount);
    expect(
      collectMeshes(world.group).every((mesh) =>
        geometries.has(mesh.geometry.uuid),
      ),
    ).toBe(true);
    world.dispose();
  });

  it("animates the portal surface through its shader without rebuilding geometry", () => {
    const world = buildSiteWorld("portal");
    const portal = collectMeshes(world.group).find(
      (mesh) => mesh.name === "Portal_Energy_Surface",
    );
    expect(portal).toBeDefined();
    const material = portal!.material as ShaderMaterial;
    expect(material.uniforms.uTime).toBeDefined();
    world.animate(0, false);
    const initial = material.uniforms.uTime!.value;
    world.animate(1.5, false);
    expect(material.uniforms.uTime!.value).not.toBe(initial);
    world.dispose();
  });
});

describe("voxel mesher", () => {
  it("culls hidden faces inside a sealed cube", () => {
    const { result, resources } = fixture((builder) => {
      builder.fill(0, 0, 0, 2, 2, 2, PAL.stone);
    });
    expect(result.quads).toBe(24);
    expect(result.triangles).toBe(48);
    resources.dispose();
  });

  it("darkens corners that touch neighbouring blocks", () => {
    const open = new VoxelBuilder();
    open.fill(0, 0, 0, 3, 1, 3, PAL.stone);
    const openMinimum = topFaceMinimum(open);

    const shaded = new VoxelBuilder();
    shaded.fill(0, 0, 0, 3, 1, 3, PAL.stone);
    shaded.set(1, 1, 1, PAL.stone);
    const shadedMinimum = topFaceMinimum(shaded);

    expect(shadedMinimum).toBeLessThan(openMinimum * 0.95);
  });
});
