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
      setClearColor() {}
      setPixelRatio() {}
      setSize() {}
      render() {
        if (state.failRender) throw new Error("first frame failed");
      }
      dispose = state.dispose;
      forceContextLoss = state.lose;
    },
  };
});
import { mountScene } from "./runtime";

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
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
    createRadialGradient: () => ({ addColorStop() {} }),
    fillRect() {},
  } as unknown as CanvasRenderingContext2D);
});
afterEach(() => vi.unstubAllGlobals());

describe("renderer resources", () => {
  const host = () => {
    const element = document.createElement("div");
    vi.spyOn(element, "getBoundingClientRect").mockReturnValue({
      width: 600,
      height: 500,
    } as DOMRect);
    return element;
  };
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
  it("disposes resources once, and handles a lost context", () => {
    const fallback = vi.fn(),
      element = host();
    const handle = mountScene(element, "portal", 30, fallback);
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
  it("builds the same procedural scene for site and launcher modes", () => {
    const siteReady = vi.fn();
    const siteScene = mountScene(
      host(),
      "portal",
      30,
      vi.fn(),
      "site",
      siteReady,
    );
    expect(siteReady).toHaveBeenCalledOnce();
    siteScene.dispose();

    const launcherReady = vi.fn();
    const launcherScene = mountScene(
      host(),
      "portal",
      30,
      vi.fn(),
      "launcher",
      launcherReady,
    );
    expect(launcherReady).toHaveBeenCalledOnce();
    launcherScene.dispose();
  });
});
