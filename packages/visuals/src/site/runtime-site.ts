import {
  ACESFilmicToneMapping,
  DataTexture,
  DirectionalLight,
  DoubleSide,
  HemisphereLight,
  Mesh,
  MeshBasicMaterial,
  OrthographicCamera,
  PCFShadowMap,
  PlaneGeometry,
  PointLight,
  RGBAFormat,
  SRGBColorSpace,
  Scene,
  UnsignedByteType,
  Vector3,
  WebGLRenderer,
} from "three";
import { createFrameLoop } from "../frame-loop";
import { clamp, easeOutCubic } from "./motion";
import { buildSiteWorld } from "./world";
import type { DioramaVariant } from "../types";

export interface SceneHandle {
  setRunning: (running: boolean) => void;
  dispose: () => void;
}

interface CameraPreset {
  position: [number, number, number];
  target: [number, number, number];
  halfHeight: number;
  shadowY: number;
}

const PRESETS: Record<DioramaVariant, CameraPreset> = {
  island: {
    position: [7.9, 6.6, 9.9],
    target: [0, 0.75, 0],
    halfHeight: 4.2,
    shadowY: -3.3,
  },
  cave: {
    position: [6.9, 4.6, 10.4],
    target: [0, 0.8, -0.4],
    halfHeight: 3.8,
    shadowY: -2.5,
  },
  portal: {
    position: [7.2, 4.7, 10.4],
    target: [0, 0.85, 0],
    halfHeight: 3.3,
    shadowY: -2.45,
  },
};

function createContactShadow() {
  const size = 128;
  const pixels = new Uint8Array(size * size * 4);
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const dx = ((x + 0.5) / size) * 2 - 1;
      const dy = ((y + 0.5) / size) * 2 - 1;
      const distance = Math.min(1, Math.hypot(dx, dy));
      const alpha = Math.pow(1 - distance, 1.4) * 0.5 * 255;
      const offset = (y * size + x) * 4;
      pixels[offset] = 12;
      pixels[offset + 1] = 16;
      pixels[offset + 2] = 14;
      pixels[offset + 3] = alpha;
    }
  }
  const texture = new DataTexture(
    pixels,
    size,
    size,
    RGBAFormat,
    UnsignedByteType,
  );
  texture.name = "ml-site-contact-shadow";
  texture.colorSpace = SRGBColorSpace;
  texture.needsUpdate = true;
  return texture;
}

export function mountScene(
  host: HTMLElement,
  variant: DioramaVariant,
  fps: number,
  onFallback: () => void,
  _mode: "site" | "launcher" = "site",
  onReady?: () => void,
): SceneHandle {
  const preset = PRESETS[variant];
  const renderer = new WebGLRenderer({
    alpha: true,
    antialias: true,
    powerPreference: "low-power",
  });
  const cleanups: Array<() => void> = [
    () => {
      renderer.dispose();
      renderer.forceContextLoss();
      renderer.domElement.remove();
    },
  ];
  let disposed = false;
  const dispose = () => {
    if (disposed) return;
    disposed = true;
    for (const cleanup of cleanups.reverse()) {
      try {
        cleanup();
      } catch (error) {
        console.warn("Site diorama cleanup failed", error);
      }
    }
  };

  try {
    renderer.outputColorSpace = SRGBColorSpace;
    renderer.toneMapping = ACESFilmicToneMapping;
    renderer.toneMappingExposure = 1.03;
    renderer.setClearColor(0x141716, 0);
    renderer.shadowMap.enabled = true;
    renderer.shadowMap.type = PCFShadowMap;
    renderer.shadowMap.autoUpdate = false;

    const scene = new Scene();
    const world = buildSiteWorld(
      variant,
      renderer.capabilities.getMaxAnisotropy(),
    );
    cleanups.push(() => world.dispose());
    scene.add(world.group);

    const camera = new OrthographicCamera(-1, 1, 1, -1, 0.1, 60);
    const target = new Vector3(...preset.target);
    camera.position.set(...preset.position);
    camera.lookAt(target);

    const ambient = new HemisphereLight("#e7ead9", "#4b4a3d", 0.8);
    scene.add(ambient);
    const sun = new DirectionalLight("#ffe4bb", 3);
    sun.position.set(-4.5, 8.5, 5.5);
    sun.castShadow = true;
    sun.shadow.mapSize.set(2048, 2048);
    Object.assign(sun.shadow.camera, {
      left: -5.5,
      right: 5.5,
      top: 5.5,
      bottom: -5.5,
      near: 0.5,
      far: 30,
    });
    sun.shadow.normalBias = 0.022;
    sun.shadow.bias = -0.0003;
    scene.add(sun);
    cleanups.push(() => sun.shadow.dispose());
    const rim = new DirectionalLight("#9fd0bb", 0.85);
    rim.position.set(5, 4, -6);
    scene.add(rim);
    const bounce = new DirectionalLight("#a7b8aa", 0.45);
    bounce.position.set(2, -4, 6);
    scene.add(bounce);
    const accent = new PointLight(
      variant === "portal"
        ? "#b45cff"
        : variant === "cave"
          ? "#7fe0c7"
          : "#ffc98a",
      variant === "portal" ? 3.2 : variant === "cave" ? 1.2 : 1.1,
      6,
      2,
    );
    accent.position.set(
      0,
      variant === "portal" ? 1.5 : 1.1,
      variant === "cave" ? -0.6 : 0.7,
    );
    scene.add(accent);

    const shadowTexture = createContactShadow();
    const shadowGeometry = new PlaneGeometry(8.8, 7.8);
    const shadowMaterial = new MeshBasicMaterial({
      map: shadowTexture,
      transparent: true,
      depthWrite: false,
      opacity: 0.7,
      toneMapped: false,
      side: DoubleSide,
    });
    cleanups.push(() => {
      shadowGeometry.dispose();
      shadowMaterial.dispose();
      shadowTexture.dispose();
    });
    const shadow = new Mesh(shadowGeometry, shadowMaterial);
    shadow.rotation.x = -Math.PI / 2;
    shadow.position.y = preset.shadowY;
    scene.add(shadow);

    host.append(renderer.domElement);
    renderer.domElement.setAttribute("aria-hidden", "true");

    let pointerX = 0;
    let pointerY = 0;
    let currentX = 0;
    let currentY = 0;
    let lastTime = 0;
    let ready = false;
    let pixelRatio = Math.min(
      devicePixelRatio || 1,
      innerWidth <= 720 ? 1 : 1.5,
    );

    const render = (elapsed: number) => {
      const delta = lastTime ? clamp(elapsed - lastTime, 0, 0.05) : 0.016;
      lastTime = elapsed;
      const follow = 1 - Math.exp(-12 * delta);
      currentX += (pointerX - currentX) * follow;
      currentY += (pointerY - currentY) * follow;
      const settle = easeOutCubic(elapsed / 1.15);
      camera.position.set(
        preset.position[0] +
          (1 - settle) * 1.05 +
          currentX * 0.7 +
          Math.sin(elapsed * 0.22) * 0.28,
        preset.position[1] +
          (1 - settle) * 0.32 +
          currentY * 0.46 +
          Math.cos(elapsed * 0.22) * 0.07,
        preset.position[2] +
          (1 - settle) * 1.05 -
          currentX * 0.34 +
          Math.cos(elapsed * 0.22) * 0.18,
      );
      camera.lookAt(target);
      if (elapsed < 1.3) renderer.shadowMap.needsUpdate = true;
      world.animate(elapsed, false);
      renderer.render(scene, camera);
      if (!ready) {
        ready = true;
        onReady?.();
      }
    };

    const resize = () => {
      if (disposed) return;
      const { width, height } = host.getBoundingClientRect();
      if (width < 1 || height < 1) return;
      renderer.setPixelRatio(pixelRatio);
      renderer.setSize(width, height, false);
      world.setPixelRatio(pixelRatio);
      const aspect = width / height;
      const halfHeight =
        aspect < 1 ? preset.halfHeight / aspect : preset.halfHeight;
      camera.left = -halfHeight * aspect;
      camera.right = halfHeight * aspect;
      camera.top = halfHeight;
      camera.bottom = -halfHeight;
      camera.updateProjectionMatrix();
      renderer.shadowMap.needsUpdate = true;
      render(0);
    };

    renderer.compile(scene, camera);
    const loop = createFrameLoop({
      fps,
      render,
      simplify() {
        pixelRatio = 1;
        renderer.shadowMap.enabled = false;
        world.setPixelRatio(1);
        resize();
      },
      fallback: onFallback,
    });
    cleanups.push(() => loop.stop());

    const pointer = (event: PointerEvent) => {
      const bounds = host.getBoundingClientRect();
      pointerX = (event.clientX - bounds.left) / bounds.width - 0.5;
      pointerY = (event.clientY - bounds.top) / bounds.height - 0.5;
    };
    const leave = () => {
      pointerX = 0;
      pointerY = 0;
    };
    const lost = (event: Event) => {
      event.preventDefault();
      loop.stop();
      onFallback();
    };
    host.addEventListener("pointermove", pointer);
    host.addEventListener("pointerleave", leave);
    renderer.domElement.addEventListener("webglcontextlost", lost);
    cleanups.push(() => {
      host.removeEventListener("pointermove", pointer);
      host.removeEventListener("pointerleave", leave);
      renderer.domElement.removeEventListener("webglcontextlost", lost);
    });

    const observer = new ResizeObserver(resize);
    cleanups.push(() => observer.disconnect());
    observer.observe(host);
    resize();

    return {
      setRunning(running: boolean) {
        if (!disposed) {
          if (running) loop.start();
          else loop.stop();
        }
      },
      dispose,
    };
  } catch (error) {
    dispose();
    throw error;
  }
}
