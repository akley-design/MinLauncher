import {
  act,
  cleanup,
  fireEvent,
  render,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { StrictMode } from "react";
import { Diorama } from "./Diorama";

const runtime = vi.hoisted(() => ({ mount: vi.fn() }));
vi.mock("./runtime", () => ({ mountScene: runtime.mount }));
let reduced = false;
let fail: (() => void) | undefined;
let ready: (() => void) | undefined;
let observers: Array<(entries: unknown[]) => void>;
let handles: Array<{
  setRunning: ReturnType<typeof vi.fn>;
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
      const handle = { setRunning: vi.fn(), dispose: vi.fn() };
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

describe("diorama lifecycle", () => {
  it("updates the site's frame budget when the viewport becomes compact", async () => {
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
    render(<Diorama variant="island" />);
    await waitFor(() => expect(handles).toHaveLength(1));
    expect(runtime.mount.mock.calls[0]![2]).toBe(60);
    act(() => {
      compactMedia.matches = true;
      compactMedia.addEventListener.mock.calls[0]![1]();
    });
    await waitFor(() => expect(handles).toHaveLength(2));
    expect(runtime.mount.mock.calls[1]![2]).toBe(30);
    expect(handles[0]!.dispose).toHaveBeenCalledOnce();
  });
  it("does not start a renderer when the launcher mounts unfocused", async () => {
    vi.spyOn(document, "hasFocus").mockReturnValue(false);
    render(<Diorama variant="island" mode="launcher" />);
    await act(async () => {});
    expect(runtime.mount).not.toHaveBeenCalled();
    fireEvent(window, new Event("focus"));
    await waitFor(() => expect(handles).toHaveLength(1));
    fireEvent(window, new Event("blur"));
    await waitFor(() => expect(handles[0]!.dispose).toHaveBeenCalledOnce());
  });

  it("stops and resumes the launcher scene on native window focus events", async () => {
    render(<Diorama variant="island" mode="launcher" />);
    await waitFor(() => expect(handles).toHaveLength(1));

    fireEvent(
      window,
      new CustomEvent("minlauncher:window-focus", { detail: false }),
    );
    await waitFor(() => expect(handles[0]!.dispose).toHaveBeenCalledOnce());

    fireEvent(
      window,
      new CustomEvent("minlauncher:window-focus", { detail: true }),
    );
    await waitFor(() => expect(handles).toHaveLength(2));
  });
  it("runs without exposing a manual animation control on the site", async () => {
    const { unmount } = render(
      <StrictMode>
        <Diorama variant="island" />
      </StrictMode>,
    );
    await waitFor(() => expect(handles).toHaveLength(1));
    expect(handles[0]!.setRunning).toHaveBeenCalledWith(true);
    unmount();
    expect(handles[0]!.dispose).toHaveBeenCalledOnce();
  });
  it("uses the shared procedural renderer on the site and in the launcher", async () => {
    const site = render(<Diorama variant="portal" mode="site" />);
    await waitFor(() => expect(handles).toHaveLength(1));
    expect(runtime.mount.mock.calls[0]![4]).toBe("site");
    site.unmount();

    render(<Diorama variant="portal" mode="launcher" />);
    await waitFor(() => expect(handles).toHaveLength(2));
    expect(runtime.mount.mock.calls[1]![4]).toBe("launcher");
  });
  it("uses the poster for reduced motion without loading WebGL", async () => {
    reduced = true;
    render(<Diorama variant="cave" />);
    await act(async () => {});
    expect(runtime.mount).not.toHaveBeenCalled();
  });
  it("keeps the site poster visible until the model finishes loading", async () => {
    const { container } = render(<Diorama variant="island" mode="site" />);
    await waitFor(() => expect(handles).toHaveLength(1));
    expect(container.firstElementChild?.getAttribute("data-rendered")).toBe(
      "false",
    );
    act(() => ready?.());
    expect(container.firstElementChild?.getAttribute("data-rendered")).toBe(
      "true",
    );
  });
  it("disposes a launcher scene when the game starts", async () => {
    const view = render(<Diorama variant="island" mode="launcher" />);
    await waitFor(() => expect(handles).toHaveLength(1));
    view.rerender(<Diorama variant="island" mode="launcher" motion="paused" />);
    await waitFor(() => expect(handles[0]!.dispose).toHaveBeenCalledOnce());
  });
  it("replaces a lost context or slow scene with its poster", async () => {
    const { container } = render(<Diorama variant="portal" />);
    await waitFor(() => expect(handles).toHaveLength(1));
    act(() => fail?.());
    await waitFor(() => expect(handles[0]!.dispose).toHaveBeenCalledOnce());
    expect(container.firstElementChild?.getAttribute("data-rendered")).toBe(
      "false",
    );
  });
});
