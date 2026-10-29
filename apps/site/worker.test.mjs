import assert from "node:assert/strict";
import test from "node:test";
import worker from "./public/_worker.js";

const files = new Map([
  ["/index.html", "<main>home</main>"],
  ["/features/index.html", "<main>features</main>"],
  ["/404.html", "<main>missing</main>"],
  ["/assets/app.js", "console.log('app')"],
]);

function assets(seen = []) {
  return {
    async fetch(request) {
      const url = new URL(request.url);
      seen.push({
        pathname: url.pathname,
        method: request.method,
        etag: request.headers.get("if-none-match"),
      });
      const body = files.get(url.pathname);
      return body === undefined
        ? new Response("Not found", { status: 404 })
        : new Response(request.method === "HEAD" ? null : body, {
            status: 200,
          });
    },
  };
}

test("worker serves prerendered HTML, redirects and real 404s", async () => {
  const seen = [];
  const env = { ASSETS: assets(seen) };
  const base = "https://minlauncher.pages.dev";

  const home = await worker.fetch(
    new Request(`${base}/`, { headers: { Accept: "text/html" } }),
    env,
  );
  assert.equal(home.status, 200);
  assert.equal(await home.text(), "<main>home</main>");
  assert.equal(home.headers.get("cache-control"), "no-cache");
  assert.match(home.headers.get("content-security-policy"), /default-src/);

  const features = await worker.fetch(
    new Request(`${base}/features`, {
      headers: { Accept: "text/html", "If-None-Match": '"old"' },
    }),
    env,
  );
  assert.equal(features.status, 200);
  assert.equal(await features.text(), "<main>features</main>");
  assert.equal(seen.at(-1).pathname, "/features/index.html");
  assert.equal(seen.at(-1).etag, null, "HTML fetch must ignore stale ETag");

  const head = await worker.fetch(
    new Request(`${base}/features`, { method: "HEAD" }),
    env,
  );
  assert.equal(head.status, 200);
  assert.equal(await head.text(), "");

  const slash = await worker.fetch(
    new Request(`${base}/features/?from=test`, { redirect: "manual" }),
    env,
  );
  assert.equal(slash.status, 308);
  assert.equal(slash.headers.get("location"), "/features?from=test");

  const index = await worker.fetch(
    new Request(`${base}/index.html`, { redirect: "manual" }),
    env,
  );
  assert.equal(index.status, 308);
  assert.equal(index.headers.get("location"), "/");

  const missing = await worker.fetch(
    new Request(`${base}/missing-page`, { headers: { Accept: "text/html" } }),
    env,
  );
  assert.equal(missing.status, 404);
  assert.equal(await missing.text(), "<main>missing</main>");
  assert.equal(missing.headers.get("x-robots-tag"), "noindex");

  const asset = await worker.fetch(new Request(`${base}/assets/app.js`), env);
  assert.equal(asset.status, 200);
  assert.equal(
    asset.headers.get("cache-control"),
    "public, max-age=31536000, immutable",
  );

  const clock = await worker.fetch(new Request(`${base}/api/clock.json`), env);
  assert.equal(clock.status, 200);
  assert.equal(
    clock.headers.get("cache-control"),
    "no-store, no-cache, must-revalidate",
  );
  const value = await clock.json();
  assert.equal(typeof value.now, "number");
  assert.equal(typeof value.releaseAt, "number");
});
