import {
  ACESFilmicToneMapping,
  CanvasTexture,
  DirectionalLight,
  HemisphereLight,
  Mesh,
  MeshBasicMaterial,
  PointLight,
  OrthographicCamera,
  PlaneGeometry,
  Scene,
  SRGBColorSpace,
  WebGLRenderer,
  Vector3,
  PCFShadowMap,
} from "three";
import { buildWorld } from "./models";
import { createFrameLoop } from "./frame-loop";
import type { DioramaVariant } from "./types";

export interface SceneHandle {
  setRunning: (running: boolean) => void;
  dispose: () => void;
}

export function mountScene(
  host: HTMLElement,
  variant: DioramaVariant,
  fps: number,
  onFallback: () => void,
  mode: "site" | "launcher" = "launcher",
  onReady?: () => void,
): SceneHandle {
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
        console.warn("Diorama resource cleanup failed", error);
      }
    }
  };
  try {
    renderer.outputColorSpace = SRGBColorSpace;
    renderer.toneMapping = ACESFilmicToneMapping;
    renderer.toneMappingExposure = 1.12;
    renderer.setClearColor(0x141716, 0);
    renderer.shadowMap.enabled = true;
    renderer.shadowMap.type = PCFShadowMap;
    renderer.shadowMap.autoUpdate = false;
    const scene = new Scene();
    let world: ReturnType<typeof buildWorld> | undefined;
    world = buildWorld(variant, renderer.capabilities.getMaxAnisotropy());
    cleanups.push(() => world?.dispose());
    scene.add(world.group);
    const camera = new OrthographicCamera(-4.7, 4.7, 4.7, -4.7, 0.1, 60);
    const target = new Vector3(0, 0.55, 0);
    camera.position.set(8, 6.5, 10);
    camera.lookAt(target);
    const ambient = new HemisphereLight(
      "#e3e8d4",
      "#4e4a39",
      mode === "site" ? 1.35 : 1.8,
    );
    scene.add(ambient);
    const sun = new DirectionalLight("#ffe2ad", mode === "site" ? 2.8 : 3.2);
    cleanups.push(() => sun.shadow.dispose());
    sun.position.set(-4, 8, 5);
    sun.castShadow = true;
    sun.shadow.mapSize.set(1024, 1024);
    Object.assign(sun.shadow.camera, {
      left: -5,
      right: 5,
      top: 5,
      bottom: -5,
      far: 25,
    });
    sun.shadow.normalBias = mode === "site" ? 0.018 : 0.035;
    sun.shadow.bias = -0.0001;
    scene.add(sun);
    const rim = new DirectionalLight("#93c6ad", 0.7);
    rim.position.set(4, 3, -6);
    scene.add(rim);
    if (mode === "site") {
      const accent = new PointLight(
        variant === "portal"
          ? "#be63ff"
          : variant === "cave"
            ? "#81eed6"
            : "#ffd397",
        variant === "portal" ? 8 : variant === "cave" ? 5 : 2.5,
        5,
        2,
      );
      accent.position.set(
        variant === "island" ? -0.9 : 0,
        variant === "portal" ? 1.7 : 1.2,
        variant === "cave" ? 1.35 : 0.8,
      );
      scene.add(accent);
    }

    const shadowCanvas = document.createElement("canvas");
    shadowCanvas.width = shadowCanvas.height = 128;
    const ctx = shadowCanvas.getContext("2d");
    if (ctx) {
      const gradient = ctx.createRadialGradient(64, 64, 0, 64, 64, 64);
      gradient.addColorStop(0, "rgba(0,0,0,.4)");
      gradient.addColorStop(1, "rgba(0,0,0,0)");
      ctx.fillStyle = gradient;
      ctx.fillRect(0, 0, 128, 128);
    }
    const texture = new CanvasTexture(shadowCanvas);
    const shadowGeometry = new PlaneGeometry(9, 8);
    const shadowMaterial = new MeshBasicMaterial({
      map: texture,
      transparent: true,
      depthWrite: false,
    });
    cleanups.push(() => {
      shadowGeometry.dispose();
      shadowMaterial.dispose();
      texture.dispose();
    });
    const shadow = new Mesh(shadowGeometry, shadowMaterial);
    shadow.rotation.x = -Math.PI / 2;
    shadow.position.y = -2.25;
    scene.add(shadow);

    host.append(renderer.domElement);
    renderer.domElement.setAttribute("aria-hidden", "true");
    let pointerX = 0,
      pointerY = 0,
      currentX = 0,
      currentY = 0;
    let pixelRatio = Math.min(
      devicePixelRatio || 1,
      innerWidth <= 720 ? 1 : 1.5,
    );
    const render = (time: number) => {
      currentX += (pointerX - currentX) * 0.035;
      currentY += (pointerY - currentY) * 0.035;
      camera.position.set(
        8 + currentX * 0.7 + Math.sin(time * 0.22) * 0.3,
        6.5 + currentY * 0.45 + Math.cos(time * 0.22) * 0.08,
        10 - currentX * 0.35 + Math.cos(time * 0.22) * 0.22,
      );
      camera.lookAt(target);
      world?.animate(time, false);
      renderer.render(scene, camera);
    };
    const resize = () => {
      if (disposed) return;
      const { width, height } = host.getBoundingClientRect();
      if (width < 1 || height < 1) return;
      renderer.setPixelRatio(pixelRatio);
      renderer.setSize(width, height, false);
      const aspect = width / height;
      const halfHeight = aspect < 1 ? 4.4 / aspect : 4.35;
      camera.left = -halfHeight * aspect;
      camera.right = halfHeight * aspect;
      camera.top = halfHeight;
      camera.bottom = -halfHeight;
      camera.updateProjectionMatrix();
      renderer.shadowMap.needsUpdate = true;
      render(0);
    };
    const loop = createFrameLoop({
      fps,
      render,
      simplify() {
        pixelRatio = 1;
        renderer.shadowMap.enabled = false;
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
      pointerX = pointerY = 0;
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
    onReady?.();
    return {
      setRunning(running) {
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
