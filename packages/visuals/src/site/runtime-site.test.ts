import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const state = vi.hoisted(() => ({
  failRender: false,
  dispose: vi.fn(),
  lose: vi.fn(),
  disconnect: vi.fn(),
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
      compile() {}
      render() {
        if (state.failRender) throw new Error("first frame failed");
      }
      dispose = state.dispose;
      forceContextLoss = state.lose;
    },
  };
});

import { mountScene } from "./runtime-site";

beforeEach(() => {
  state.failRender = false;
  state.dispose.mockClear();
  state.lose.mockClear();
  state.disconnect.mockClear();
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

describe("site renderer resources", () => {
  it("rolls back a failed first frame, including the canvas and observer", () => {
    state.failRender = true;
    const element = host();
    expect(() => mountScene(element, "island", 30, vi.fn())).toThrow(
      "first frame failed",
    );
    expect(element.querySelector("canvas")).toBeNull();
    expect(state.dispose).toHaveBeenCalledOnce();
    expect(state.lose).toHaveBeenCalledOnce();
    expect(state.disconnect).toHaveBeenCalledOnce();
  });

  it("reports readiness once and hands a lost context back to the poster", () => {
    const fallback = vi.fn();
    const ready = vi.fn();
    const element = host();
    const handle = mountScene(element, "portal", 30, fallback, "site", ready);
    expect(ready).toHaveBeenCalledOnce();
    element
      .querySelector("canvas")!
      .dispatchEvent(new Event("webglcontextlost", { cancelable: true }));
    expect(fallback).toHaveBeenCalledOnce();
    handle.dispose();
    handle.dispose();
    expect(element.querySelector("canvas")).toBeNull();
    expect(state.dispose).toHaveBeenCalledOnce();
    expect(state.disconnect).toHaveBeenCalledOnce();
  });

  it("builds and starts every site scene", () => {
    for (const variant of ["island", "cave", "portal"] as const) {
      const element = host();
      const ready = vi.fn();
      const handle = mountScene(element, variant, 60, vi.fn(), "site", ready);
      expect(ready).toHaveBeenCalledOnce();
      handle.setRunning(true);
      handle.setRunning(false);
      handle.dispose();
    }
  });
});
