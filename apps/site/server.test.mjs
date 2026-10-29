import assert from "node:assert/strict";
import { get } from "node:http";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { gunzipSync } from "node:zlib";
import test from "node:test";
import { createSiteServer } from "./server.mjs";

test("serves deep links with SPA fallback, serves assets, and blocks traversal", async () => {
  const temp = await mkdtemp(join(tmpdir(), "minlaucher-site-"));
  const testDist = join(temp, "dist");
  await mkdir(join(testDist, "assets"), { recursive: true });
  await writeFile(join(testDist, "index.html"), "<main>site</main>");
  const assetBody = "x".repeat(512);
  await writeFile(join(testDist, "assets", "app.js"), assetBody);
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
    const home = await fetch(`${base}/news/speed-without-magic`, {
      headers: { Accept: "text/html" },
    });
    assert.equal(home.status, 200);
    assert.equal(await home.text(), "<main>site</main>");
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
