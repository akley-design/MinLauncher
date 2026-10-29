export interface FrameLoopOptions {
  fps: number;
  render: (elapsed: number) => void;
  simplify: () => boolean | void;
  restore?: () => void;
  fallback: () => void;
  adaptive?: boolean;
  warmupMs?: number;
  windowMs?: number;
  slowRatio?: number;
  goodWindows?: number;
}

export function createFrameLoop({
  fps,
  render,
  simplify,
  restore,
  fallback,
  adaptive = false,
  warmupMs = 3500,
  windowMs = 2000,
  slowRatio = 0.75,
  goodWindows = 2,
}: FrameLoopOptions) {
  let frame: number | undefined;
  let previous = 0,
    started = 0,
    measuredAt = 0,
    count = 0,
    slowWindows = 0,
    good = 0;
  let warmupDone = false;
  let atFloor = false;
  let dead = false;
  let interval = 1000 / fps;

  const tick = (now: number) => {
    frame = undefined;
    if (dead) return;
    const delta = now - previous;
    if (delta >= interval - 0.5) {
      previous = delta < interval ? now : now - (delta % interval);
      try {
        render((now - started) / 1000);
      } catch (error) {
        dead = true;
        console.warn("MinLauncher frame failed", error);
        fallback();
        return;
      }
      count++;
      if (adaptive) {
        if (!warmupDone) {
          if (now - started >= warmupMs) {
            warmupDone = true;
            measuredAt = now;
            count = 0;
          }
        } else if (now - measuredAt >= windowMs) {
          const actualFps = (count * 1000) / (now - measuredAt);
          measuredAt = now;
          count = 0;
          if (actualFps < fps * slowRatio) {
            slowWindows++;
            good = 0;
            if (!atFloor) {
              if (simplify() === false) atFloor = true;
            } else if (slowWindows >= 2) {
              dead = true;
              fallback();
              return;
            }
          } else if (actualFps >= fps * 0.9) {
            slowWindows = 0;
            good++;
            if (good >= goodWindows) {
              good = 0;
              restore?.();
              atFloor = false;
            }
          }
        }
      } else if (now - measuredAt >= 3000) {
        const actualFps = (count * 1000) / (now - measuredAt);
        slowWindows = actualFps < 24 ? slowWindows + 1 : 0;
        measuredAt = now;
        count = 0;
        if (slowWindows === 1) simplify();
        if (slowWindows >= 2) {
          dead = true;
          fallback();
          return;
        }
      }
    }
    if (!dead) frame = requestAnimationFrame(tick);
  };

  return {
    start() {
      if (frame !== undefined || dead) return;
      const now = performance.now();
      if (!adaptive || !started) started = now;
      previous = measuredAt = now;
      count = 0;
      slowWindows = 0;
      good = 0;
      warmupDone = false;
      atFloor = false;
      frame = requestAnimationFrame(tick);
    },
    stop() {
      if (frame !== undefined) cancelAnimationFrame(frame);
      frame = undefined;
    },
    setFps(next: number) {
      if (next > 0) interval = 1000 / next;
    },
  };
}
