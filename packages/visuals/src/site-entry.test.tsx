import {
  act,
  cleanup,
  fireEvent,
  render,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { StrictMode } from "react";
import { SiteDiorama } from "./site-entry";
import { scenePostersSite } from "./site/posters";

const runtime = vi.hoisted(() => ({ mount: vi.fn() }));
vi.mock("./site/runtime-site", () => ({ mountScene: runtime.mount }));

let reduced = false;
let fail: (() => void) | undefined;
let ready: (() => void) | undefined;
let observers: Array<(entries: unknown[]) => void>;
let handles: Array<{
  setRunning: ReturnType<typeof vi.fn>;
  setActive: ReturnType<typeof vi.fn>;
  setQuality: ReturnType<typeof vi.fn>;
  dispose: ReturnType<typeof vi.fn>;
}>;

beforeEach(() => {
  reduced = false;
  observers = [];
  handles = [];
  fail = undefined;
  ready = undefined;
  vi.spyOn(document, "hasFocus").mockReturnValue(true);
  vi.stubGlobal("matchMedia", () => ({
    matches: reduced,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  }));
  vi.stubGlobal("WebGLRenderingContext", class {});
  vi.stubGlobal(
    "IntersectionObserver",
    class {
      constructor(callback: (entries: unknown[]) => void) {
        observers.push(callback);
      }
      observe() {
        observers.at(-1)?.([{ isIntersecting: true, intersectionRatio: 1 }]);
      }
      disconnect() {}
    },
  );
  runtime.mount.mockImplementation(
    (_host, _variant, _fps, fallback, _mode, onReady) => {
      fail = fallback;
      ready = onReady;
      const handle = {
        setRunning: vi.fn(),
        setActive: vi.fn(),
        setQuality: vi.fn(),
        dispose: vi.fn(),
      };
      handles.push(handle);
      return handle;
    },
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  runtime.mount.mockReset();
});

describe("site diorama entry", () => {
  it("loads the site renderer with the site budget and poster", async () => {
    const { container } = render(<SiteDiorama variant="island" />);
    await waitFor(() => expect(handles).toHaveLength(1));
    expect(runtime.mount.mock.calls[0]![4]).toBe("site");
    expect(runtime.mount.mock.calls[0]![2]).toBe(60);
    expect(handles[0]!.setRunning).toHaveBeenCalledWith(true);
    expect(handles[0]!.setActive).toHaveBeenCalledWith(true);
    expect(container.querySelector("img")?.getAttribute("src")).toBe(
      scenePostersSite.island,
    );
  });

  it("drops to the compact frame budget without rebuilding the scene", async () => {
    const compactMedia = {
      matches: false,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    };
    vi.stubGlobal("matchMedia", (query: string) =>
      query.includes("max-width")
        ? compactMedia
        : {
            matches: false,
            addEventListener: vi.fn(),
            removeEventListener: vi.fn(),
          },
    );
    render(<SiteDiorama variant="island" />);
    await waitFor(() => expect(handles).toHaveLength(1));
    expect(runtime.mount.mock.calls[0]![2]).toBe(60);
    act(() => {
      compactMedia.matches = true;
      compactMedia.addEventListener.mock.calls[0]![1]();
    });
    await waitFor(() =>
      expect(handles[0]!.setQuality).toHaveBeenCalledWith({
        fps: 30,
        compact: true,
      }),
    );
    expect(handles).toHaveLength(1);
    expect(handles[0]!.dispose).not.toHaveBeenCalled();
  });

  it("keeps the poster for reduced motion without loading WebGL", async () => {
    reduced = true;
    render(<SiteDiorama variant="cave" />);
    await act(async () => {});
    expect(runtime.mount).not.toHaveBeenCalled();
  });

  it("returns to the poster when the context is lost", async () => {
    const { container } = render(<SiteDiorama variant="portal" />);
    await waitFor(() => expect(handles).toHaveLength(1));
    expect(container.firstElementChild?.getAttribute("data-rendered")).toBe(
      "false",
    );
    act(() => ready?.());
    expect(container.firstElementChild?.getAttribute("data-rendered")).toBe(
      "true",
    );
    act(() => fail?.());
    await waitFor(() => expect(handles[0]!.dispose).toHaveBeenCalledOnce());
    expect(container.firstElementChild?.getAttribute("data-rendered")).toBe(
      "false",
    );
  });

  it("mounts under StrictMode without exposing a manual control", async () => {
    const { container, unmount } = render(
      <StrictMode>
        <SiteDiorama variant="island" />
      </StrictMode>,
    );
    await waitFor(() => expect(handles.length).toBeGreaterThan(0));
    expect(container.querySelector(".ml-diorama__control")).toBeNull();
    unmount();
    expect(handles[0]!.dispose).toHaveBeenCalledOnce();
  });

  it("pauses and resumes the scene without disposing it", async () => {
    const view = render(<SiteDiorama variant="island" />);
    await waitFor(() => expect(handles).toHaveLength(1));
    view.rerender(<SiteDiorama variant="island" motion="paused" />);
    await waitFor(() =>
      expect(handles[0]!.setRunning).toHaveBeenLastCalledWith(false),
    );
    expect(handles[0]!.setActive).toHaveBeenLastCalledWith(false);
    expect(handles[0]!.dispose).not.toHaveBeenCalled();
    view.rerender(<SiteDiorama variant="island" />);
    await waitFor(() =>
      expect(handles[0]!.setRunning).toHaveBeenLastCalledWith(true),
    );
    expect(handles[0]!.setActive).toHaveBeenLastCalledWith(true);
  });
});
