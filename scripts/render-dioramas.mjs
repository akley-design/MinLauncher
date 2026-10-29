import { chromium } from "playwright";
import { execFileSync } from "node:child_process";
import { writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = fileURLToPath(new URL("../", import.meta.url));
const origin = process.env.ML_SITE_URL || "http://127.0.0.1:5173";
const runtimeUrl = `/@fs/${path.join(root, "packages/visuals/src/runtime.ts").replaceAll("\\", "/")}`;
const siteRuntimeUrl = `/@fs/${path.join(root, "packages/visuals/src/site/runtime-site.ts").replaceAll("\\", "/")}`;
// The committed site posters are WebP; ImageMagick converts the transparent
// PNG renders so the fallback images stay small.
function toWebp(png) {
  execFileSync(
    "magick",
    [
      png,
      "-quality",
      "92",
      "-define",
      "webp:alpha-quality=100",
      png.replace(/\.png$/, ".webp"),
    ],
    { stdio: "inherit" },
  );
}

function deriveArt(sourcePng, target, width, height, resize, gravity, quality) {
  execFileSync(
    "magick",
    [
      sourcePng,
      "-trim",
      "+repage",
      "-resize",
      resize,
      "-background",
      "none",
      "-gravity",
      gravity,
      "-extent",
      `${width}x${height}`,
      "-quality",
      String(quality),
      "-define",
      "webp:alpha-quality=100",
      target,
    ],
    { stdio: "inherit" },
  );
}

const icon = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48" fill="none"><path d="m24 2 20 11v22L24 46 4 35V13Z" fill="#B6D879"/><path d="m24 26 20-13v22L24 46Z" fill="#78994F"/><path d="M11 17v15l6 3V23l7 7 7-7v12l6-3V17l-6-3-7 8-7-8Z" fill="#182019"/><path d="m4 13 20 13L44 13M24 26v20" stroke="#141716" stroke-opacity=".15"/></svg>`;
const browser = await chromium.launch({
  headless: true,
  ...(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE
    ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE }
    : {}),
});
try {
  const page = await browser.newPage({
    viewport: { width: 1200, height: 1000 },
    deviceScaleFactor: 1,
  });
  await page.route("**/design-capture", (route) =>
    route.fulfill({
      contentType: "text/html",
      body: '<html><body style="margin:0;background:transparent"><div id="stage" style="width:1200px;height:1000px"></div></body></html>',
    }),
  );
  await page.goto(`${origin}/design-capture`);
  for (const variant of ["island", "cave", "portal"]) {
    await page.evaluate(
      async ({ variant, runtimeUrl }) => {
        window.scene?.dispose();
        const { mountScene } = await import(runtimeUrl);
        window.scene = mountScene(
          document.getElementById("stage"),
          variant,
          30,
          () => {},
        );
      },
      { variant, runtimeUrl },
    );
    await page.screenshot({
      path: path.join(root, `packages/visuals/assets/${variant}.png`),
      omitBackground: true,
    });
    console.log(`Rendered legacy ${variant}`);
  }

  await page.evaluate(() => window.scene?.dispose());
  await page.setViewportSize({ width: 1200, height: 1200 });
  await page.evaluate(() => {
    const stage = document.getElementById("stage");
    stage.style.width = "1200px";
    stage.style.height = "1200px";
  });
  for (const variant of ["island", "cave", "portal"]) {
    await page.evaluate(
      async ({ variant, siteRuntimeUrl }) => {
        window.scene?.dispose();
        const { mountScene } = await import(siteRuntimeUrl);
        window.scene = mountScene(
          document.getElementById("stage"),
          variant,
          30,
          () => {},
          "site",
        );
        window.scene.setRunning(true);
      },
      { variant, siteRuntimeUrl },
    );
    await page.waitForTimeout(1900);
    const sitePng = path.join(
      root,
      `packages/visuals/assets/${variant}-site.png`,
    );
    await page.screenshot({ path: sitePng, omitBackground: true });
    toWebp(sitePng);
    console.log(`Rendered site ${variant}`);
  }

  await page.setViewportSize({ width: 320, height: 320 });
  await page.evaluate(() => {
    const stage = document.getElementById("stage");
    stage.style.width = "320px";
    stage.style.height = "320px";
  });
  await page.evaluate(
    async ({ siteRuntimeUrl }) => {
      window.scene?.dispose();
      const { mountScene } = await import(siteRuntimeUrl);
      window.scene = mountScene(
        document.getElementById("stage"),
        "portal",
        30,
        () => {},
        "site",
      );
      window.scene.setRunning(true);
    },
    { siteRuntimeUrl },
  );
  await page.waitForTimeout(1500);
  const thumbPng = path.join(
    root,
    "packages/visuals/assets/portal-site-thumb.png",
  );
  await page.screenshot({ path: thumbPng, omitBackground: true });
  toWebp(thumbPng);
  await page.evaluate(() => window.scene?.dispose());

  for (const variant of ["island", "cave", "portal"]) {
    const source = path.join(
      root,
      `packages/visuals/assets/${variant}-site.png`,
    );
    const gravity =
      variant === "island" ? "East" : variant === "cave" ? "West" : "Center";
    deriveArt(
      source,
      path.join(root, `packages/visuals/assets/${variant}-wide.webp`),
      1440,
      810,
      "x760",
      gravity,
      90,
    );
    deriveArt(
      source,
      path.join(root, `packages/visuals/assets/${variant}-cover.webp`),
      1200,
      750,
      "x690",
      "Center",
      88,
    );
    console.log(`Derived ${variant} wide/cover`);
  }

  await writeFile(path.join(root, "apps/ui/public/brand.svg"), icon);
  await page.setViewportSize({ width: 512, height: 512 });
  await page.setContent(
    `<style>body{margin:0;background:transparent}svg{width:512px;height:512px}</style>${icon}`,
  );
  const source = path.join(root, "crates/ml-desktop/icons/icon-source.png");
  await page.screenshot({ path: source, omitBackground: true });
  await page.setViewportSize({ width: 1200, height: 630 });
  await page.setContent(
    `<style>@font-face{font-family:Golos;src:url('${origin}/@fs/${path.join(root, "packages/visuals/assets/GolosText.ttf").replaceAll("\\", "/")}')}*{box-sizing:border-box}body{margin:0;background:#141716;color:#f2efe7;font-family:Golos,sans-serif;width:1200px;height:630px;padding:58px 60px;overflow:hidden}.brand{display:flex;align-items:center;gap:12px;font-size:24px;font-weight:600}.brand svg{width:35px}h1{font-size:86px;line-height:1.03;font-weight:500;letter-spacing:-5px;margin:80px 0 25px;width:520px}h1 span{color:#b6d879}p{font-size:22px;color:#b2b9af;width:390px;line-height:1.6}img{position:absolute;width:680px;height:600px;object-fit:contain;right:-10px;top:45px}</style><div class="brand">${icon} MinLauncher</div><h1>Твой<br/><span>Minecraft.</span></h1><p>Версии, сборки и настройки игры в одном лаунчере.</p><img src="${origin}/@fs/${path.join(root, "packages/visuals/assets/island-site.png").replaceAll("\\", "/")}" alt="">`,
  );
  await page.evaluate(() =>
    Promise.all([
      document.fonts.ready,
      ...[...document.images].map((img) => img.decode()),
    ]),
  );
  await page.screenshot({
    path: path.join(root, "apps/ui/public/social-card.png"),
  });
} finally {
  await browser.close();
}
