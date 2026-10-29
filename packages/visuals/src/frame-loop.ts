export function createFrameLoop({
  fps,
  render,
  simplify,
  fallback,
}: {
  fps: number;
  render: (elapsed: number) => void;
  simplify: () => void;
  fallback: () => void;
}) {
  let frame: number | undefined;
  let previous = 0,
    started = 0,
    measuredAt = 0,
    count = 0,
    slowWindows = 0;
  const interval = 1000 / fps;
  const tick = (now: number) => {
    frame = undefined;
    const delta = now - previous;
    if (delta >= interval - 0.5) {
      previous = delta < interval ? now : now - (delta % interval);
      render((now - started) / 1000);
      count++;
      if (now - measuredAt >= 3000) {
        const actualFps = (count * 1000) / (now - measuredAt);
        slowWindows = actualFps < 24 ? slowWindows + 1 : 0;
        measuredAt = now;
        count = 0;
        if (slowWindows === 1) simplify();
        if (slowWindows >= 2) {
          fallback();
          return;
        }
      }
    }
    frame = requestAnimationFrame(tick);
  };
  return {
    start() {
      if (frame !== undefined) return;
      previous = started = measuredAt = performance.now();
      count = 0;
      slowWindows = 0;
      frame = requestAnimationFrame(tick);
    },
    stop() {
      if (frame !== undefined) cancelAnimationFrame(frame);
      frame = undefined;
    },
  };
}
