import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createFrameLoop } from "./frame-loop";
import { registerScene } from "./ownership";

describe("decorative render budget", () => {
  let queue: Map<number, FrameRequestCallback>;
  let id: number;
  const step = (time: number) => {
    const callbacks = [...queue.values()];
    queue.clear();
    callbacks.forEach((callback) => callback(time));
  };
  beforeEach(() => {
    queue = new Map();
    id = 0;
    vi.spyOn(performance, "now").mockReturnValue(0);
    vi.stubGlobal(
      "requestAnimationFrame",
      vi.fn((callback: FrameRequestCallback) => {
        queue.set(++id, callback);
        return id;
      }),
    );
    vi.stubGlobal(
      "cancelAnimationFrame",
      vi.fn((frame: number) => queue.delete(frame)),
    );
  });
  afterEach(() => vi.unstubAllGlobals());
  it("caps rendering and owns only one loop after repeated starts", () => {
    const render = vi.fn();
    const loop = createFrameLoop({
      fps: 30,
      render,
      simplify: vi.fn(),
      fallback: vi.fn(),
    });
    loop.start();
    loop.start();
    expect(queue.size).toBe(1);
    step(16);
    expect(render).not.toHaveBeenCalled();
    step(34);
    step(50);
    step(67);
    expect(render).toHaveBeenCalledTimes(2);
    loop.stop();
    expect(queue.size).toBe(0);
    step(100);
    expect(render).toHaveBeenCalledTimes(2);
    loop.start();
    expect(queue.size).toBe(1);
    loop.stop();
  });
  it("keeps the cap when display timestamps land just below a frame boundary", () => {
    const render = vi.fn();
    const loop = createFrameLoop({
      fps: 30,
      render,
      simplify: vi.fn(),
      fallback: vi.fn(),
    });
    loop.start();
    for (const time of [16.6, 33.2, 49.8, 66.4, 83]) step(time);
    expect(render).toHaveBeenCalledTimes(2);
    loop.stop();
  });
  it("simplifies sustained slow rendering before stopping for the poster", () => {
    const simplify = vi.fn(),
      fallback = vi.fn();
    const loop = createFrameLoop({
      fps: 30,
      render: vi.fn(),
      simplify,
      fallback,
    });
    loop.start();
    for (let time = 100; time <= 3000; time += 100) step(time);
    expect(simplify).toHaveBeenCalledTimes(1);
    expect(fallback).not.toHaveBeenCalled();
    for (let time = 3100; time <= 6000; time += 100) step(time);
    expect(fallback).toHaveBeenCalledOnce();
    expect(queue.size).toBe(0);
  });
  it("elects one visible scene and returns ownership when it leaves", () => {
    const first = vi.fn(),
      second = vi.fn();
    const a = registerScene(first),
      b = registerScene(second);
    a.update(0.8);
    b.update(0.4);
    expect(first).toHaveBeenLastCalledWith(true);
    expect(second).toHaveBeenLastCalledWith(false);
    a.update(0);
    expect(first).toHaveBeenLastCalledWith(false);
    expect(second).toHaveBeenLastCalledWith(true);
    b.dispose();
    a.dispose();
  });
});
