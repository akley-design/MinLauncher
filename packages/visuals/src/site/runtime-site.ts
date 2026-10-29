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
import type {
  DioramaVariant,
  SceneHandle,
  SceneMountOptions,
  SceneQuality,
} from "../types";

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
    target: [0, 0.7, 0],
    halfHeight: 2.8,
    shadowY: -2.45,
  },
};

interface QualityStep {
  dpr: number;
  shadows: boolean;
}

const DESKTOP_QUALITY: QualityStep[] = [
  { dpr: 1.5, shadows: true },
  { dpr: 1.25, shadows: true },
  { dpr: 1, shadows: true },
  { dpr: 1, shadows: false },
  { dpr: 0.85, shadows: false },
];

const COMPACT_QUALITY: QualityStep[] = [
  { dpr: 1.25, shadows: false },
  { dpr: 1, shadows: false },
  { dpr: 0.85, shadows: false },
];

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

function createRenderer(compact: boolean) {
  const options = {
    alpha: true,
    antialias: !compact,
  } as const;
  try {
    return new WebGLRenderer({
      ...options,
      powerPreference: "high-performance",
    });
  } catch {
    return new WebGLRenderer({
      ...options,
      antialias: false,
      powerPreference: "low-power",
    });
  }
}

export function mountScene(
  host: HTMLElement,
  variant: DioramaVariant,
  fps: number,
  onFallback: () => void,
  _mode: "site" | "launcher" = "site",
  onReady?: () => void,
  options: SceneMountOptions = {},
): SceneHandle {
  const preset = PRESETS[variant];
  let compact = options.compact ?? innerWidth <= 720;
  const adaptive = options.adaptive ?? true;
  const perfWindow = window as Window & {
    __mlPerfFrames?: Record<string, number>;
  };
  const perfFrames =
    typeof location !== "undefined" &&
    new URLSearchParams(location.search).has("ml-perf")
      ? (perfWindow.__mlPerfFrames ??= {})
      : null;
  let frameCount = 0;
  const renderer = createRenderer(compact);
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
    renderer.shadowMap.enabled = !compact;
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
    sun.castShadow = !compact;
    sun.shadow.mapSize.set(compact ? 1024 : 2048, compact ? 1024 : 2048);
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

    renderer.domElement.setAttribute("aria-hidden", "true");
    let attached = false;
    let compiled = false;
    let runningRequested = false;
    let activeRequested = options.attach !== false;
    let ready = false;
    let firstRender = true;
    let shadowDirty = true;
    let shadowSettled = false;
    let lastTime = 0;
    let lastWidth = 0;
    let lastHeight = 0;
    let lastPixelRatio = 0;
    let bounds: DOMRect | null = null;
    let boundsFrame = 0;

    let quality = compact ? COMPACT_QUALITY : DESKTOP_QUALITY;
    let qualityIndex = 0;
    let pixelRatio = quality[0]!.dpr;

    let pointerX = 0;
    let pointerY = 0;
    let currentX = 0;
    let currentY = 0;

    const refreshBounds = () => {
      bounds = host.getBoundingClientRect();
    };

    const scheduleBounds = () => {
      if (boundsFrame) return;
      boundsFrame = requestAnimationFrame(() => {
        boundsFrame = 0;
        refreshBounds();
      });
    };

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
      if (!shadowSettled && elapsed >= 1.15) {
        shadowSettled = true;
        shadowDirty = true;
      }
      if (firstRender) {
        firstRender = false;
        shadowDirty = true;
      }
      if (shadowDirty && renderer.shadowMap.enabled) {
        renderer.shadowMap.needsUpdate = true;
      }
      shadowDirty = false;
      world.animate(elapsed, false);
      renderer.render(scene, camera);
      if (perfFrames) perfFrames[variant] = ++frameCount;
      if (!ready) {
        ready = true;
        onReady?.();
      }
    };

    const resize = (force = false) => {
      if (disposed) return;
      const rect = host.getBoundingClientRect();
      const width = Math.round(rect.width);
      const height = Math.round(rect.height);
      if (width < 1 || height < 1) return;
      bounds = rect;
      if (
        !force &&
        width === lastWidth &&
        height === lastHeight &&
        pixelRatio === lastPixelRatio
      )
        return;
      lastWidth = width;
      lastHeight = height;
      lastPixelRatio = pixelRatio;
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
      shadowDirty = true;
    };

    const applyQuality = () => {
      const step = quality[qualityIndex]!;
      pixelRatio = step.dpr;
      renderer.shadowMap.enabled = step.shadows;
      sun.castShadow = step.shadows;
      world.setPixelRatio(pixelRatio);
      resize(true);
      if (step.shadows) shadowDirty = true;
    };

    const attach = () => {
      if (disposed || attached) return;
      host.append(renderer.domElement);
      attached = true;
      refreshBounds();
      resize(true);
    };

    const detach = () => {
      if (!attached) return;
      renderer.domElement.remove();
      attached = false;
    };

    const loop = createFrameLoop({
      fps,
      render,
      adaptive,
      simplify() {
        if (qualityIndex < quality.length - 1) {
          qualityIndex++;
          applyQuality();
          return true;
        }
        return false;
      },
      restore() {
        if (qualityIndex > 0) {
          qualityIndex--;
          applyQuality();
        }
      },
      fallback: onFallback,
    });
    cleanups.push(() => loop.stop());

    const pointer = (event: PointerEvent) => {
      if (!bounds) refreshBounds();
      if (!bounds || bounds.width === 0 || bounds.height === 0) return;
      pointerX = (event.clientX - bounds.left) / bounds.width - 0.5;
      pointerY = (event.clientY - bounds.top) / bounds.height - 0.5;
    };
    const leave = () => {
      pointerX = 0;
      pointerY = 0;
    };
    const enter = () => refreshBounds();
    const lost = (event: Event) => {
      event.preventDefault();
      loop.stop();
      onFallback();
    };
    const scrolled = () => scheduleBounds();
    host.addEventListener("pointermove", pointer, { passive: true });
    host.addEventListener("pointerenter", enter, { passive: true });
    host.addEventListener("pointerleave", leave, { passive: true });
    window.addEventListener("scroll", scrolled, { passive: true });
    renderer.domElement.addEventListener("webglcontextlost", lost);
    cleanups.push(() => {
      host.removeEventListener("pointermove", pointer);
      host.removeEventListener("pointerenter", enter);
      host.removeEventListener("pointerleave", leave);
      window.removeEventListener("scroll", scrolled);
      renderer.domElement.removeEventListener("webglcontextlost", lost);
      if (boundsFrame) cancelAnimationFrame(boundsFrame);
    });

    const observer = new ResizeObserver(() => {
      scheduleBounds();
      resize();
    });
    cleanups.push(() => observer.disconnect());
    observer.observe(host);
    resize(true);

    const compile = () => {
      const targetRenderer = renderer as WebGLRenderer & {
        compileAsync?: (
          scene: Scene,
          camera: OrthographicCamera,
        ) => Promise<unknown>;
      };
      return typeof targetRenderer.compileAsync === "function"
        ? targetRenderer.compileAsync(scene, camera)
        : Promise.resolve().then(() => renderer.compile(scene, camera));
    };
    void compile()
      .then(() => {
        if (disposed) return;
        compiled = true;
        if (activeRequested && runningRequested) loop.start();
      })
      .catch((error: unknown) => {
        if (disposed) return;
        console.warn("MinLauncher diorama compile failed", error);
        onFallback();
      });

    if (options.attach !== false) attach();

    return {
      setRunning(running: boolean) {
        if (disposed) return;
        runningRequested = running;
        if (running && compiled && attached) loop.start();
        else loop.stop();
      },
      setActive(active: boolean) {
        if (disposed) return;
        activeRequested = active;
        if (active) {
          attach();
          if (compiled && runningRequested) loop.start();
        } else {
          loop.stop();
          detach();
        }
      },
      setQuality(next: SceneQuality) {
        if (disposed) return;
        loop.setFps(next.fps);
        if (next.compact !== compact) {
          compact = next.compact;
          quality = compact ? COMPACT_QUALITY : DESKTOP_QUALITY;
          qualityIndex = Math.min(qualityIndex, quality.length - 1);
          applyQuality();
        }
      },
      dispose,
    };
  } catch (error) {
    dispose();
    throw error;
  }
}
