import { launcherFixture } from "./design-fixture.mjs";
import { chromium } from "playwright";
import assert from "node:assert/strict";
import { mkdir, writeFile, copyFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = fileURLToPath(new URL("../", import.meta.url));
const output = path.join(root, "docs/living-dioramas");
await mkdir(output, { recursive: true });
const browser = await chromium.launch({
  headless: true,
  ...(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE
    ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE }
    : {}),
  args: ["--enable-precise-memory-info"],
});
const findings = { screenshots: [], errors: [], layout: [], measurements: {} };
const siteUrl = process.env.ML_SITE_URL || "http://127.0.0.1:5173";
const uiUrl = process.env.ML_UI_URL || "http://127.0.0.1:1420";
const siteOnly = process.env.ML_DESIGN_SITE_ONLY === "1";

async function shot(page, name, fullPage = false) {
  await page.screenshot({
    path: path.join(output, `${name}.png`),
    fullPage,
    animations: "disabled",
  });
  findings.screenshots.push(name);
}
async function inspect(page, name) {
  const result = await page.evaluate(() => ({
    width: innerWidth,
    scroll: document.documentElement.scrollWidth,
    canvases: document.querySelectorAll("canvas").length,
  }));
  assert.ok(
    result.scroll <= result.width + 1,
    `${name}: horizontal overflow ${JSON.stringify(result)}`,
  );
  assert.ok(result.canvases <= 1, `${name}: more than one canvas`);
  findings.layout.push({ name, ...result });
}
function errors(page) {
  page.on("pageerror", (error) => findings.errors.push(error.message));
}

async function siteReady(page) {
  await page.waitForTimeout(500);
  if (await page.locator(".ml-diorama").count())
    await page
      .locator('.ml-diorama[data-rendered="true"]')
      .first()
      .waitFor({ timeout: 15000 })
      .catch(() => {});
}

async function siteMeta(page, name, expectedPage) {
  const data = await page.evaluate(() => ({
    title: document.title,
    page: document.querySelector(".site")?.getAttribute("data-page"),
    mains: document.querySelectorAll("main").length,
    inline: document.querySelectorAll("[style]:not(canvas)").length,
    canvasInline: document.querySelectorAll("canvas[style]").length,
    active: [
      ...document.querySelectorAll('.site-nav > a[aria-current="page"]'),
    ].map((link) => link.textContent?.trim()),
  }));
  assert.equal(data.mains, 1, `${name}: main count ${data.mains}`);
  assert.equal(data.inline, 0, `${name}: inline styles ${data.inline}`);
  assert.ok(data.title.length > 0, `${name}: empty document title`);
  assert.equal(data.page, expectedPage, `${name}: page identity`);
  if (!["home", "not-found"].includes(expectedPage))
    assert.ok(data.active.length > 0, `${name}: no active navigation tab`);
  findings.layout.push({ name, ...data });
}

try {
  if (!siteOnly) {
    const launcher = await browser.newPage({
      viewport: { width: 1180, height: 760 },
    });
    await launcherFixture(launcher);
    errors(launcher);
    await launcher.goto(uiUrl, { waitUntil: "networkidle" });
    await launcher
      .getByRole("button", { name: "Играть", exact: true })
      .waitFor();
    await launcher
      .locator('.ml-diorama[data-rendered="true"]')
      .waitFor({ timeout: 20000 });
    assert.equal(
      await launcher.locator(".home-hero .ml-diorama__control").count(),
      0,
      "launcher home should not show a manual animation control",
    );
    await shot(launcher, "launcher-home");
    await copyFile(
      path.join(output, "launcher-home.png"),
      path.join(root, "apps/ui/public/launcher-preview.png"),
    );
    for (const [width, height] of [
      [940, 600],
      [1180, 760],
      [1920, 1080],
    ]) {
      await launcher.setViewportSize({ width, height });
      await inspect(launcher, `launcher-${width}`);
      const play = await launcher
        .getByRole("button", { name: "Играть", exact: true })
        .boundingBox();
      assert.ok(
        play && play.y + play.height <= height,
        `play below fold ${width}`,
      );
      if (width === 1920) {
        const hero = await launcher.locator(".home-hero").boundingBox();
        assert.ok(
          hero && hero.height > 700,
          "maximized home hero did not grow",
        );
      }
      await shot(launcher, `launcher-home-${width}`);
    }
    await launcher.setViewportSize({ width: 1180, height: 760 });
    for (const [route, label] of [
      ["versions", "Версии"],
      ["instances", "Инстансы"],
      ["downloads", "Загрузки"],
      ["performance", "Производительность"],
      ["settings", "Настройки"],
    ]) {
      await launcher.getByRole("link", { name: label, exact: true }).click();
      await launcher
        .getByRole("heading", { name: label, exact: true })
        .waitFor();
      await inspect(launcher, `launcher-${route}`);
      await shot(launcher, `launcher-${route}`);
    }
    await launcher.getByLabel("Максимальная куча").fill("6G");
    await shot(launcher, "launcher-settings-unsaved");
    await launcher
      .getByRole("button", { name: "Сохранить", exact: true })
      .click();
    await launcher.getByText("Сохранено", { exact: true }).waitFor();
    await launcher.getByRole("link", { name: "Играть", exact: true }).click();
    await launcher.getByRole("button", { name: "Играть", exact: true }).click();
    await launcher.getByRole("button", { name: /Остановить игру/ }).waitFor();
    await launcher.locator("canvas").waitFor({ state: "detached" });
    await shot(launcher, "launcher-running");
    await launcher.getByRole("button", { name: /Остановить игру/ }).click();
    await launcher.locator('.ml-diorama[data-rendered="true"]').waitFor();
    await launcher.evaluate(() =>
      window.__designEmit("minlauncher://install", {
        kind: "started",
        version_id: "1.20.1",
        total: 100,
        bytes: 32000000,
      }),
    );
    await shot(launcher, "launcher-installing");
    await launcher.evaluate(() =>
      window.__designEmit("minlauncher://install", {
        kind: "finished",
        version_id: "1.20.1",
        files: 100,
        bytes: 32000000,
        elapsed_ms: 1000,
      }),
    );
    const cdp = await launcher.context().newCDPSession(launcher);
    await cdp.send("HeapProfiler.collectGarbage");
    const before = await cdp.send("Runtime.getHeapUsage");
    for (let i = 0; i < 20; i++) {
      await launcher.getByRole("link", { name: "Версии", exact: true }).click();
      await launcher.getByRole("link", { name: "Играть", exact: true }).click();
      await launcher.locator('.ml-diorama[data-rendered="true"]').waitFor();
    }
    await cdp.send("HeapProfiler.collectGarbage");
    const after = await cdp.send("Runtime.getHeapUsage");
    findings.measurements.navigationHeap = {
      before: before.usedSize,
      after: after.usedSize,
      growth: after.usedSize - before.usedSize,
    };
    assert.ok(
      after.usedSize - before.usedSize < 5 * 1024 * 1024,
      "heap growth after navigation exceeds 5 MiB",
    );
    for (let i = 0; i < 20; i++) {
      await launcher.getByRole("link", { name: "Версии", exact: true }).click();
      await launcher.getByRole("link", { name: "Играть", exact: true }).click();
      await launcher.locator('.ml-diorama[data-rendered="true"]').waitFor();
    }
    await cdp.send("HeapProfiler.collectGarbage");
    const warmed = await cdp.send("Runtime.getHeapUsage");
    findings.measurements.navigationHeap.warmedGrowth =
      warmed.usedSize - after.usedSize;
    assert.ok(
      warmed.usedSize - after.usedSize < 1024 * 1024,
      "heap did not stabilize after warmup",
    );
    await launcher.evaluate(() => window.dispatchEvent(new Event("blur")));
    await launcher.locator("canvas").waitFor({ state: "detached" });
    await launcher.evaluate(() => window.dispatchEvent(new Event("focus")));
    await launcher.locator('.ml-diorama[data-rendered="true"]').waitFor();
    await launcher.evaluate(() =>
      document
        .querySelector("canvas")
        .dispatchEvent(new Event("webglcontextlost", { cancelable: true })),
    );
    await launcher.locator("canvas").waitFor({ state: "detached" });
    await shot(launcher, "launcher-webgl-fallback");
    await launcher.close();

    for (const state of ["empty", "error"]) {
      const page = await browser.newPage({
        viewport: { width: 940, height: 600 },
      });
      errors(page);
      await launcherFixture(page, state);
      await page.goto(uiUrl, { waitUntil: "networkidle" });
      await shot(page, `launcher-${state}`);
      await inspect(page, `launcher-${state}`);
      await page.close();
    }
    for (const factor of [1.25, 1.5]) {
      const context = await browser.newContext({
        viewport: { width: 940, height: 600 },
        deviceScaleFactor: factor,
      });
      const page = await context.newPage();
      errors(page);
      await launcherFixture(page);
      await page.goto(uiUrl, { waitUntil: "networkidle" });
      await shot(page, `launcher-scale-${factor}`);
      await inspect(page, `launcher-scale-${factor}`);
      await context.close();
    }
  }

  const expectedPages = {
    home: "home",
    features: "features",
    download: "download",
    news: "news",
    article: "article",
    account: "account",
    profile: "settings",
    reset: "reset",
    callback: "callback",
    "not-found": "not-found",
  };
  const site = await browser.newPage();
  errors(site);
  await site.route("**/api/clock.json", (route) =>
    route.fulfill({ json: { now: Date.now(), releaseAt: 0 } }),
  );
  await site.route("**/latest.json", (route) =>
    route.fulfill({
      json: {
        version: "0.1.0",
        pub_date: "2026-10-25T00:00:00Z",
        platforms: {
          "windows-x86_64-nsis": {
            url: "https://github.com/akley-design/MinLauncher/releases/download/v0.1.0/setup.exe",
            signature: "test",
          },
        },
      },
    }),
  );
  for (const width of [360, 390, 768, 1024, 1440]) {
    await site.setViewportSize({ width, height: width < 600 ? 844 : 1000 });
    for (const [route, name] of [
      ["/", "home"],
      ["/features", "features"],
      ["/download", "download"],
      ["/news", "news"],
      ["/news/speed-without-magic", "article"],
      ["/account", "account"],
      ["/account/settings", "profile"],
      ["/auth/reset-password", "reset"],
      ["/auth/callback", "callback"],
      ["/missing-page", "not-found"],
    ]) {
      await site.goto(siteUrl + route, { waitUntil: "domcontentloaded" });
      await siteReady(site);
      await inspect(site, `site-${name}-${width}`);
      await siteMeta(site, `site-${name}-${width}`, expectedPages[name]);
      await shot(site, `site-${name}-${width}`, width === 1440 || width < 600);
    }
  }
  await site.setViewportSize({ width: 390, height: 844 });
  await site.goto(siteUrl, { waitUntil: "domcontentloaded" });
  await siteReady(site);
  await site.getByRole("button", { name: "Открыть меню" }).click();
  await site.keyboard.press("Escape");
  assert.equal(
    await site
      .getByRole("button", { name: "Открыть меню" })
      .getAttribute("aria-expanded"),
    "false",
  );
  await site.emulateMedia({ reducedMotion: "reduce" });
  await site.reload({ waitUntil: "domcontentloaded" });
  await site.waitForTimeout(600);
  assert.equal(await site.locator("canvas").count(), 0);
  await shot(site, "site-reduced-motion");
  await site.emulateMedia({ reducedMotion: "no-preference" });
  await site.setViewportSize({ width: 1440, height: 1000 });
  await site.goto(siteUrl, { waitUntil: "domcontentloaded" });
  await siteReady(site);
  await site.waitForTimeout(1500);
  await shot(site, "site-home-preview");
  await site.locator(".feature-story").scrollIntoViewIfNeeded();
  await site.waitForTimeout(1400);
  await shot(site, "site-home-story-1440");
  await site.unroute("**/latest.json");
  for (const state of ["loading", "unpublished", "error"]) {
    await site.route("**/latest.json", (route) =>
      state === "loading"
        ? new Promise((resolve) => {
            setTimeout(() => {
              void route
                .fulfill({ status: 404, body: "Not published" })
                .catch(() => {})
                .finally(resolve);
            }, 1500);
          })
        : route.fulfill({
            status: state === "unpublished" ? 404 : 503,
            body: "Unavailable",
          }),
    );
    await site.goto(siteUrl + "/download", {
      waitUntil: "domcontentloaded",
    });
    const status =
      state === "loading"
        ? "Проверяем последний релиз…"
        : state === "unpublished"
          ? "Установщик ещё не опубликован"
          : "Не удалось проверить релиз";
    await site.getByRole("status").filter({ hasText: status }).waitFor();
    await shot(site, `site-download-${state}`);
    await site.unroute("**/latest.json");
  }
  await site.close();
  assert.deepEqual(findings.errors, [], "browser page errors");
  console.log(
    JSON.stringify(
      {
        screenshots: findings.screenshots.length,
        layouts: findings.layout.length,
        errors: findings.errors,
        measurements: findings.measurements,
      },
      null,
      2,
    ),
  );
} finally {
  await writeFile(
    path.join(output, "browser-report.json"),
    JSON.stringify(findings, null, 2),
  );
  await browser.close();
}
