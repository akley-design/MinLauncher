import assert from "node:assert/strict";
import { get } from "node:http";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { gunzipSync } from "node:zlib";
import test from "node:test";
import { createSiteServer } from "./server.mjs";

test("serves static routes, real 404s, redirects, assets and releases", async () => {
  const temp = await mkdtemp(join(tmpdir(), "minlaucher-site-"));
  const testDist = join(temp, "dist");
  await mkdir(join(testDist, "assets"), { recursive: true });
  await mkdir(join(testDist, "features"), { recursive: true });
  await mkdir(join(testDist, "news", "speed-without-magic"), {
    recursive: true,
  });
  await mkdir(join(testDist, "media"), { recursive: true });
  await writeFile(join(testDist, "index.html"), "<main>site</main>");
  await writeFile(
    join(testDist, "features", "index.html"),
    "<main>features</main>",
  );
  await writeFile(
    join(testDist, "news", "speed-without-magic", "index.html"),
    "<main>article</main>",
  );
  await writeFile(join(testDist, "404.html"), "<main>missing</main>");
  const assetBody = "x".repeat(512);
  await writeFile(join(testDist, "assets", "app.js"), assetBody);
  await writeFile(join(testDist, "media", "scene.avif"), assetBody);
  let hasRelease = true;
  let releaseFailed = false;
  let releaseMethod;
  const fetchRelease = async (url, options) => {
    assert.equal(
      url,
      "https://github.com/akley-design/MinLauncher/releases/latest/download/latest.json",
    );
    releaseMethod = options.method;
    if (releaseFailed) throw new Error("Network unavailable");
    return hasRelease
      ? new Response('{"version":"1.2.3"}', { status: 200 })
      : new Response("Not found", { status: 404 });
  };
  const server = createSiteServer(testDist, fetchRelease);
  const address = await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => resolve(server.address()));
  });
  try {
    const base = `http://127.0.0.1:${address.port}`;
    const home = await fetch(`${base}/`, { headers: { Accept: "text/html" } });
    assert.equal(home.status, 200);
    assert.equal(await home.text(), "<main>site</main>");
    assert.equal(home.headers.get("cache-control"), "no-cache");

    const article = await fetch(`${base}/news/speed-without-magic`, {
      headers: { Accept: "text/html" },
    });
    assert.equal(article.status, 200);
    assert.equal(await article.text(), "<main>article</main>");

    const features = await fetch(`${base}/features`, {
      headers: { Accept: "text/html" },
    });
    assert.equal(features.status, 200);
    assert.equal(await features.text(), "<main>features</main>");

    const slash = await fetch(`${base}/features/?from=test`, {
      redirect: "manual",
      headers: { Accept: "text/html" },
    });
    assert.equal(slash.status, 308);
    assert.equal(slash.headers.get("location"), "/features?from=test");

    const indexHtml = await fetch(`${base}/index.html`, {
      redirect: "manual",
    });
    assert.equal(indexHtml.status, 308);
    assert.equal(indexHtml.headers.get("location"), "/");

    const missing = await fetch(`${base}/missing-page`, {
      headers: { Accept: "text/html" },
    });
    assert.equal(missing.status, 404);
    assert.equal(await missing.text(), "<main>missing</main>");
    assert.equal(missing.headers.get("x-robots-tag"), "noindex");

    const missingAsset = await fetch(`${base}/missing.js`);
    assert.equal(missingAsset.status, 404);
    assert.equal(await missingAsset.text(), "Not found");

    const asset = await new Promise((resolve, reject) =>
      get(
        `${base}/assets/app.js`,
        { headers: { "Accept-Encoding": "gzip" } },
        (response) => {
          const chunks = [];
          response.on("data", (chunk) => chunks.push(chunk));
          response.on("end", () =>
            resolve({ response, body: Buffer.concat(chunks) }),
          );
          response.on("error", reject);
        },
      ).on("error", reject),
    );
    assert.equal(asset.response.statusCode, 200);
    assert.equal(asset.response.headers["content-encoding"], "gzip");
    assert.equal(
      asset.response.headers["cache-control"],
      "public, max-age=31536000, immutable",
    );
    assert.equal(gunzipSync(asset.body).toString(), assetBody);

    const media = await fetch(`${base}/media/scene.avif`);
    assert.equal(media.status, 200);
    assert.equal(media.headers.get("content-type"), "image/avif");
    assert.equal(media.headers.get("cache-control"), "public, max-age=604800");

    const headAsset = await fetch(`${base}/assets/app.js`, {
      method: "HEAD",
      headers: { "Accept-Encoding": "gzip" },
    });
    assert.equal(headAsset.status, 200);
    assert.equal(headAsset.headers.get("content-encoding"), "gzip");
    assert.equal(
      headAsset.headers.get("content-type"),
      "text/javascript; charset=utf-8",
    );
    assert.equal(await headAsset.text(), "");

    const manifest = await fetch(`${base}/api/latest.json`);
    assert.equal(manifest.status, 200);
    assert.equal(
      manifest.headers.get("content-type"),
      "application/json; charset=utf-8",
    );
    assert.equal(
      manifest.headers.get("content-security-policy").includes("unsafe-inline"),
      false,
    );
    assert.equal(await manifest.text(), '{"version":"1.2.3"}');
    const headManifest = await fetch(`${base}/api/latest.json`, {
      method: "HEAD",
    });
    assert.equal(headManifest.status, 200);
    assert.equal(releaseMethod, "HEAD");
    assert.equal(await headManifest.text(), "");
    hasRelease = false;
    const missingRelease = await fetch(`${base}/api/latest.json`);
    assert.equal(missingRelease.status, 404);
    assert.equal(await missingRelease.text(), "Release is not published");
    releaseFailed = true;
    const failedRelease = await fetch(`${base}/api/latest.json`);
    assert.equal(failedRelease.status, 502);
    assert.equal(await failedRelease.text(), "Release manifest unavailable");
    const traversal = await fetch(`${base}/..%2fserver.mjs`);
    assert.equal(traversal.status, 403);
  } finally {
    server.close();
    await rm(temp, { recursive: true, force: true });
  }
});
