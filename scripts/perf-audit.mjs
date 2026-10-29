import { chromium } from "playwright";
import { createSiteServer } from "../apps/site/server.mjs";

const executablePath = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE;
const browser = await chromium.launch({
  headless: true,
  ...(executablePath ? { executablePath } : {}),
});
const server = createSiteServer();
await new Promise((resolve, reject) => {
  server.once("error", reject);
  server.listen(0, "127.0.0.1", resolve);
});
const origin = `http://127.0.0.1:${server.address().port}`;
const failures = [];
const report = {};

function check(condition, message) {
  if (!condition) failures.push(message);
}

async function instrument(page) {
  await page.addInitScript(() => {
    const perf = {
      lcp: 0,
      fcp: 0,
      cls: 0,
      longTasks: [],
      lost: 0,
    };
    window.__mlPerf = perf;
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        if (entry.name === "first-contentful-paint") perf.fcp = entry.startTime;
      }
    }).observe({ type: "paint", buffered: true });
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries())
        if (entry.entryType === "largest-contentful-paint")
          perf.lcp = entry.startTime;
    }).observe({
      type: "largest-contentful-paint",
      buffered: true,
    });
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) perf.lcp = entry.startTime;
    }).observe({ type: "largest-contentful-paint", buffered: true });
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries())
        if (!entry.hadRecentInput) perf.cls += entry.value;
    }).observe({ type: "layout-shift", buffered: true });
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries())
        perf.longTasks.push({
          start: entry.startTime,
          duration: entry.duration,
        });
    }).observe({ type: "longtask", buffered: true });
  });
}

async function frames(page, count = 90) {
  return page.evaluate(
    (total) =>
      new Promise((resolve) => {
        const samples = [];
        let last = performance.now();
        let seen = 0;
        const tick = (now) => {
          samples.push(now - last);
          last = now;
          seen++;
          if (seen >= total) resolve(samples);
          else requestAnimationFrame(tick);
        };
        requestAnimationFrame(tick);
      }),
    count,
  );
}

function percentile(values, p) {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * p))];
}

async function measure(name, viewport, throttled) {
  const context = await browser.newContext({
    viewport,
    deviceScaleFactor: viewport.width < 600 ? 1 : 1.5,
    reducedMotion: "no-preference",
  });
  const page = await context.newPage();
  const consoleErrors = [];
  page.on("pageerror", (error) => consoleErrors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });
  await instrument(page);
  if (throttled) {
    const cdp = await context.newCDPSession(page);
    await cdp.send("Network.enable");
    await cdp.send("Network.emulateNetworkConditions", {
      offline: false,
      latency: 150,
      downloadThroughput: (1.6 * 1024 * 1024) / 8,
      uploadThroughput: (750 * 1024) / 8,
    });
    await cdp.send("Emulation.setCPUThrottlingRate", { rate: 4 });
  }
  const started = Date.now();
  await page.goto(`${origin}/?ml-perf=1`, {
    waitUntil: "domcontentloaded",
  });
  await page
    .locator('.ml-diorama[data-rendered="true"]')
    .first()
    .waitFor({ timeout: 15000 });
  await page.waitForTimeout(1200);
  const firstFrame = await frames(page);
  const renderStart = await page.evaluate(() => ({
    frames: window.__mlPerfFrames?.island ?? 0,
    time: performance.now(),
  }));
  await page.waitForTimeout(2000);
  const renderEnd = await page.evaluate(() => ({
    frames: window.__mlPerfFrames?.island ?? 0,
    time: performance.now(),
  }));
  const renderFps =
    (renderEnd.frames - renderStart.frames) /
    ((renderEnd.time - renderStart.time) / 1000);
  const resources = await page.evaluate(() =>
    performance.getEntriesByType("resource").map((entry) => ({
      name: entry.name,
      transferSize: entry.transferSize,
      initiatorType: entry.initiatorType,
    })),
  );
  const runtimeRequests = resources.filter((entry) =>
    entry.name.includes("runtime-site"),
  ).length;
  const initialScriptBytes = resources
    .filter((entry) => /\/assets\/(index|react)-[^/]+\.js$/.test(entry.name))
    .reduce((sum, entry) => sum + entry.transferSize, 0);
  const fontBytes = resources
    .filter((entry) => entry.name.endsWith(".woff2"))
    .reduce((sum, entry) => sum + entry.transferSize, 0);
  const imageBytes = resources
    .filter((entry) => /\.(avif|webp|jpe?g|png)$/i.test(entry.name))
    .reduce((sum, entry) => sum + entry.transferSize, 0);
  const perf = await page.evaluate(() => {
    const value = window.__mlPerf;
    const canvas = document.querySelector("canvas");
    if (canvas) {
      window.__mlLost = 0;
      canvas.addEventListener("webglcontextlost", () => {
        window.__mlLost++;
      });
    }
    return {
      lcp: value.lcp,
      fcp: value.fcp,
      cls: value.cls,
      longTasks: value.longTasks,
      canvasCount: document.querySelectorAll("canvas").length,
      dioramas: [...document.querySelectorAll(".ml-diorama")].map((item) => ({
        rendered: item.getAttribute("data-rendered"),
        active: item.getAttribute("data-active"),
      })),
    };
  });

  await page
    .locator(".feature-story")
    .scrollIntoViewIfNeeded()
    .catch(() => {});
  await page.waitForTimeout(1400);
  const scrolled = await page.evaluate(() => ({
    canvasCount: document.querySelectorAll("canvas").length,
    lost: window.__mlLost ?? 0,
    rendered: [...document.querySelectorAll(".ml-diorama")].map((item) =>
      item.getAttribute("data-rendered"),
    ),
    runtimeRequests: performance
      .getEntriesByType("resource")
      .filter((entry) => entry.name.includes("runtime-site")).length,
  }));
  await page.evaluate(() => scrollTo(0, 0));
  await page.waitForTimeout(800);

  const tbt = perf.longTasks
    .filter((task) => task.start > (perf.fcp || 0))
    .reduce((sum, task) => sum + Math.max(0, task.duration - 50), 0);
  const result = {
    viewport: `${viewport.width}x${viewport.height}`,
    elapsedMs: Date.now() - started,
    fcp: Math.round(perf.fcp),
    lcp: Math.round(perf.lcp),
    cls: Number(perf.cls.toFixed(4)),
    tbt: Math.round(tbt),
    longTasks: perf.longTasks.length,
    frameP50: Number(percentile(firstFrame, 0.5).toFixed(1)),
    frameP95: Number(percentile(firstFrame, 0.95).toFixed(1)),
    renderFps: Number(renderFps.toFixed(1)),
    canvasCount: perf.canvasCount,
    canvasAfterScroll: scrolled.canvasCount,
    webglLost: scrolled.lost,
    runtimeRequests,
    runtimeRequestsAfterScroll: scrolled.runtimeRequests,
    rendered: scrolled.rendered,
    initialScriptBytes,
    fontBytes,
    imageBytes,
    totalTransferBytes: resources.reduce(
      (sum, entry) => sum + entry.transferSize,
      0,
    ),
    consoleErrors,
  };
  report[name] = result;

  check(
    result.lcp > 0 && result.lcp < (throttled ? 3500 : 1800),
    `${name}: LCP ${result.lcp}ms`,
  );
  check(result.cls <= 0.02, `${name}: CLS ${result.cls}`);
  check(result.tbt <= (throttled ? 600 : 200), `${name}: TBT ${result.tbt}ms`);
  check(
    result.renderFps >= (throttled ? 24 : 48),
    `${name}: render ${result.renderFps}fps`,
  );
  check(
    result.initialScriptBytes <= 150 * 1024,
    `${name}: initial JS ${result.initialScriptBytes} bytes`,
  );
  check(
    result.fontBytes <= 70 * 1024,
    `${name}: font ${result.fontBytes} bytes`,
  );
  check(result.canvasCount <= 1, `${name}: ${result.canvasCount} canvases`);
  check(
    result.canvasAfterScroll <= 1,
    `${name}: ${result.canvasAfterScroll} canvases after scroll`,
  );
  check(result.webglLost === 0, `${name}: WebGL context lost`);
  check(
    result.runtimeRequests <= 1,
    `${name}: runtime fetched ${result.runtimeRequests} times`,
  );
  check(
    result.runtimeRequestsAfterScroll <= 1,
    `${name}: runtime fetched again after scroll`,
  );
  check(result.consoleErrors.length === 0, `${name}: console errors`);
  check(
    result.frameP95 <= (throttled ? 55 : 30),
    `${name}: p95 frame ${result.frameP95}ms`,
  );
  await context.close();
}

try {
  await measure("desktop-home", { width: 1440, height: 900 }, false);
  await measure("mobile-home", { width: 390, height: 844 }, true);

  const context = await browser.newContext({
    viewport: { width: 390, height: 844 },
    reducedMotion: "reduce",
  });
  const page = await context.newPage();
  await page.goto(`${origin}/`, { waitUntil: "domcontentloaded" });
  await page.waitForTimeout(1200);
  const result = await page.evaluate(() => ({
    canvases: document.querySelectorAll("canvas").length,
    runtime: performance
      .getEntriesByType("resource")
      .filter((entry) => entry.name.includes("runtime-site")).length,
    rendered: document.querySelectorAll('[data-rendered="true"]').length,
  }));
  report.reducedMotion = result;
  check(result.canvases === 0, "reduced motion: canvas mounted");
  check(result.runtime === 0, "reduced motion: 3D runtime loaded");
  await context.close();
} finally {
  await browser.close();
  server.close();
}

console.log(JSON.stringify({ report, failures }, null, 2));
if (failures.length) process.exitCode = 1;
