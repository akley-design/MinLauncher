import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const state = vi.hoisted(() => ({
  failCompile: false,
  dispose: vi.fn(),
  lose: vi.fn(),
  disconnect: vi.fn(),
}));

const loop = vi.hoisted(() => ({
  render: undefined as ((elapsed: number) => void) | undefined,
  simplify: undefined as (() => boolean | void) | undefined,
  restore: undefined as (() => void) | undefined,
  start: vi.fn(),
  stop: vi.fn(),
  setFps: vi.fn(),
}));

vi.mock("../frame-loop", () => ({
  createFrameLoop: (options: {
    render: (elapsed: number) => void;
    simplify: () => boolean | void;
    restore?: () => void;
  }) => {
    loop.render = options.render;
    loop.simplify = options.simplify;
    loop.restore = options.restore;
    return {
      start: loop.start,
      stop: loop.stop,
      setFps: loop.setFps,
    };
  },
}));

vi.mock("three", async () => {
  const actual = await vi.importActual<typeof import("three")>("three");
  return {
    ...actual,
    WebGLRenderer: class {
      domElement = document.createElement("canvas");
      shadowMap = {
        enabled: false,
        type: 0,
        autoUpdate: false,
        needsUpdate: false,
      };
      capabilities = { getMaxAnisotropy: () => 4 };
      outputColorSpace = "";
      toneMapping = 0;
      toneMappingExposure = 1;
      setClearColor() {}
      setPixelRatio() {}
      setSize() {}
      compile() {
        if (state.failCompile) throw new Error("compile failed");
      }
      render() {}
      dispose = state.dispose;
      forceContextLoss = state.lose;
    },
  };
});

import { mountScene } from "./runtime-site";

beforeEach(() => {
  state.failCompile = false;
  state.dispose.mockClear();
  state.lose.mockClear();
  state.disconnect.mockClear();
  loop.render = undefined;
  loop.simplify = undefined;
  loop.restore = undefined;
  loop.start.mockClear();
  loop.stop.mockClear();
  loop.setFps.mockClear();
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect = state.disconnect;
    },
  );
});

afterEach(() => vi.unstubAllGlobals());

function host() {
  const element = document.createElement("div");
  vi.spyOn(element, "getBoundingClientRect").mockReturnValue({
    width: 600,
    height: 500,
  } as DOMRect);
  return element;
}

describe("site renderer lifecycle", () => {
  it("keeps the scene alive, attaches only when active, and reports readiness once", async () => {
    const fallback = vi.fn();
    const ready = vi.fn();
    const element = host();
    const handle = mountScene(element, "portal", 30, fallback, "site", ready, {
      attach: false,
    });
    expect(element.querySelector("canvas")).toBeNull();

    handle.setActive?.(true);
    handle.setRunning(true);
    await vi.waitFor(() => expect(loop.start).toHaveBeenCalled());
    expect(element.querySelector("canvas")).not.toBeNull();

    loop.render?.(0);
    loop.render?.(0.1);
    expect(ready).toHaveBeenCalledOnce();

    handle.setRunning(false);
    expect(loop.stop).toHaveBeenCalled();
    handle.setRunning(true);
    expect(loop.start).toHaveBeenCalledTimes(2);

    handle.setActive?.(false);
    expect(loop.stop).toHaveBeenCalled();
    expect(element.querySelector("canvas")).toBeNull();

    handle.setActive?.(true);
    expect(element.querySelector("canvas")).not.toBeNull();
    handle.dispose();
    handle.dispose();
    expect(element.querySelector("canvas")).toBeNull();
    expect(state.dispose).toHaveBeenCalledOnce();
    expect(state.lose).toHaveBeenCalledOnce();
    expect(state.disconnect).toHaveBeenCalledOnce();
  });

  it("hands a lost context back to the poster without killing the page", async () => {
    const fallback = vi.fn();
    const ready = vi.fn();
    const element = host();
    const handle = mountScene(element, "portal", 30, fallback, "site", ready);
    handle.setRunning(true);
    await vi.waitFor(() => expect(loop.start).toHaveBeenCalled());
    element
      .querySelector("canvas")!
      .dispatchEvent(new Event("webglcontextlost", { cancelable: true }));
    expect(fallback).toHaveBeenCalledOnce();
    expect(loop.stop).toHaveBeenCalled();
    handle.dispose();
    expect(element.querySelector("canvas")).toBeNull();
  });

  it("reports compile failures instead of throwing asynchronously", async () => {
    state.failCompile = true;
    const fallback = vi.fn();
    const element = host();
    const handle = mountScene(element, "island", 30, fallback);
    handle.setActive?.(true);
    handle.setRunning(true);
    await vi.waitFor(() => expect(fallback).toHaveBeenCalledOnce());
    handle.dispose();
    expect(element.querySelector("canvas")).toBeNull();
  });

  it("applies frame and compact budget changes without rebuilding", () => {
    const element = host();
    const handle = mountScene(element, "island", 60, vi.fn());
    handle.setQuality?.({ fps: 30, compact: true });
    expect(loop.setFps).toHaveBeenCalledWith(30);
    handle.dispose();
  });

  it("builds every site scene", () => {
    for (const variant of ["island", "cave", "portal"] as const) {
      const element = host();
      const handle = mountScene(element, variant, 60, vi.fn());
      handle.setActive?.(true);
      handle.setRunning(true);
      handle.dispose();
    }
  });
});
