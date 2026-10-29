import {
  DataTexture,
  InstancedMesh,
  MeshLambertMaterial,
  NearestFilter,
} from "three";
import { describe, expect, it } from "vitest";
import { buildWorld } from "./models";
import type { DioramaVariant } from "./types";

describe("voxel dioramas", () => {
  it.each(["island", "cave", "portal"] as DioramaVariant[])(
    "builds a textured, animated %s scene and releases it safely",
    (variant) => {
      const world = buildWorld(variant);
      const voxelMeshes = world.group.children.filter(
        (child): child is InstancedMesh => child instanceof InstancedMesh,
      );
      expect(voxelMeshes.length).toBeGreaterThan(0);
      const textureKinds = new Set(
        voxelMeshes.map(
          (mesh) => (mesh.material as MeshLambertMaterial).map?.name,
        ),
      );
      expect(textureKinds.size).toBeGreaterThan(1);
      expect(
        voxelMeshes.every((mesh) => (mesh.material as MeshLambertMaterial).map),
      ).toBe(true);
      for (const voxelMesh of voxelMeshes) {
        const texture = (voxelMesh.material as MeshLambertMaterial).map;
        expect(texture).toBeInstanceOf(DataTexture);
        expect(texture?.magFilter).toBe(NearestFilter);
        expect(texture?.image.width).toBe(16);
      }

      world.animate(0, false);
      const initialY = world.group.position.y;
      const initialRotation = world.group.rotation.y;
      world.animate(2, false);
      expect(world.group.position.y).not.toBe(initialY);
      expect(world.group.rotation.y).not.toBe(initialRotation);

      world.animate(2, true);
      expect(
        world.group.children.some((child) => child.visible === false),
      ).toBe(true);
      world.dispose();
    },
  );

  it("animates the portal surface without rebuilding its geometry", () => {
    const world = buildWorld("portal");
    const portal = world.group.children.find(
      (child) =>
        child.type === "Mesh" && child.geometry.parameters.width === 1.64,
    );
    expect(portal).toBeDefined();
    const positions = portal!.geometry.getAttribute("position");
    const center = Math.floor(positions.count / 2);
    const initial = positions.getZ(center);
    world.animate(0, false);
    world.animate(1, false);
    expect(positions.getZ(center)).not.toBe(initial);
    world.dispose();
  });
});
