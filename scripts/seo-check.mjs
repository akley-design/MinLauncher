import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const site = join(root, "apps/site");
const dist = join(site, "dist");
const ssr = join(site, "dist-ssr/entry-server.js");

const { routes } = await import(pathToFileURL(ssr).href);

function fileFor(pathname) {
  return pathname === "/"
    ? join(dist, "index.html")
    : join(dist, pathname.slice(1), "index.html");
}

function first(html, pattern) {
  return pattern.exec(html)?.[1] ?? "";
}

function count(html, pattern) {
  return [...html.matchAll(pattern)].length;
}

function readMeta(html, type, key) {
  const pattern = new RegExp(`<meta\\s+${type}="${key}"\\s+content="([^"]*)"`);
  return first(html, pattern);
}

const titles = new Set();
const descriptions = new Set();
const canonicals = new Set();
const publicCanonicals = new Set();

for (const route of routes) {
  const html = await readFile(fileFor(route.path), "utf8");
  const title = first(html, /<title>([\s\S]*?)<\/title>/);
  const description = readMeta(html, "name", "description");
  const robots = readMeta(html, "name", "robots");
  const canonical = first(html, /<link\s+rel="canonical"\s+href="([^"]*)"/);
  const page = first(html, /data-page="([^"]+)"/);
  const h1 = count(html, /<h1[ >]/g);
  const mains = count(html, /<main[ >]/g);
  const jsonLd = [
    ...html.matchAll(
      /<script type="application\/ld\+json">([\s\S]*?)<\/script>/g,
    ),
  ];
  const preloadImage = count(html, /<link\s+rel="preload"\s+as="image"/g);

  assert.equal(mains, 1, `${route.path}: expected one <main>`);
  assert.ok(title.length > 8, `${route.path}: missing title`);
  assert.ok(description.length > 30, `${route.path}: missing description`);
  assert.ok(page, `${route.path}: missing data-page`);
  assert.ok(html.includes('<html lang="ru"'), `${route.path}: missing lang=ru`);
  assert.equal(
    count(html, /<meta name="description"/g),
    1,
    `${route.path}: duplicate description`,
  );
  assert.equal(
    count(html, /<meta property="og:title"/g),
    1,
    `${route.path}: duplicate og:title`,
  );
  assert.equal(count(html, /<link rel="canonical"/g), canonical ? 1 : 0);

  if (route.private) {
    assert.ok(
      robots.includes("noindex"),
      `${route.path}: private must be noindex`,
    );
    assert.equal(canonical, "", `${route.path}: private must not be canonical`);
    assert.equal(
      jsonLd.length,
      0,
      `${route.path}: private must not have JSON-LD`,
    );
    assert.ok(
      !html.includes('data-prerendered="public"'),
      `${route.path}: private shell must mount client-side`,
    );
  } else {
    assert.ok(
      robots.includes("index"),
      `${route.path}: public must be indexable`,
    );
    assert.ok(
      canonical.startsWith("https://minlauncher.pages.dev"),
      `${route.path}: bad canonical`,
    );
    assert.ok(h1 >= 1, `${route.path}: public page needs an h1`);
    assert.ok(jsonLd.length >= 1, `${route.path}: missing JSON-LD`);
    assert.ok(preloadImage >= 1, `${route.path}: missing image preload`);
    assert.ok(
      html.includes('data-prerendered="public"'),
      `${route.path}: missing prerender marker`,
    );
    const ogImage = readMeta(html, "property", "og:image");
    assert.ok(
      ogImage.startsWith("https://minlauncher.pages.dev/"),
      `${route.path}: bad og:image`,
    );
    const imagePath = new URL(ogImage).pathname;
    assert.ok(
      existsSync(join(dist, imagePath)),
      `${route.path}: missing ${imagePath}`,
    );
    assert.ok(
      readMeta(html, "name", "twitter:card"),
      `${route.path}: missing twitter card`,
    );
    assert.ok(
      readMeta(html, "property", "og:image:alt"),
      `${route.path}: missing og:image:alt`,
    );
    for (const block of jsonLd) {
      const value = JSON.parse(block[1]);
      assert.equal(
        value["@context"],
        "https://schema.org",
        `${route.path}: bad JSON-LD context`,
      );
      assert.ok(
        Array.isArray(value["@graph"]),
        `${route.path}: JSON-LD graph missing`,
      );
    }
    titles.add(title);
    descriptions.add(description);
    canonicals.add(canonical);
    publicCanonicals.add(canonical);
  }
}

assert.equal(
  titles.size,
  routes.filter((route) => !route.private).length,
  "titles must be unique",
);
assert.equal(
  descriptions.size,
  routes.filter((route) => !route.private).length,
  "descriptions must be unique",
);
assert.equal(
  canonicals.size,
  routes.filter((route) => !route.private).length,
  "canonicals must be unique",
);

const notFound = await readFile(join(dist, "404.html"), "utf8");
assert.equal(count(notFound, /<main[ >]/g), 1, "404: expected one <main>");
assert.ok(
  readMeta(notFound, "name", "robots").includes("noindex"),
  "404: must be noindex",
);
assert.ok(first(notFound, /<title>([\s\S]*?)<\/title>/), "404: missing title");

const sitemap = await readFile(join(dist, "sitemap.xml"), "utf8");
const locs = new Set(
  [...sitemap.matchAll(/<loc>([^<]+)<\/loc>/g)].map((match) => match[1]),
);
assert.deepEqual(
  [...locs].sort(),
  [...publicCanonicals].sort(),
  "sitemap must equal public canonicals",
);
assert.ok(
  !sitemap.includes("/account"),
  "sitemap must not contain private pages",
);

const robots = await readFile(join(dist, "robots.txt"), "utf8");
assert.ok(
  robots.includes("Sitemap: https://minlauncher.pages.dev/sitemap.xml"),
  "robots: missing sitemap",
);
assert.ok(
  robots.includes("Disallow: /account"),
  "robots: private pages should be disallowed",
);

const feed = await readFile(join(dist, "feed.xml"), "utf8");
assert.ok(feed.includes('<rss version="2.0">'), "feed: not RSS");
assert.ok(feed.includes("/news/beta-0-1-1"), "feed: missing articles");

for (const file of [
  "fonts/GolosText-subset.woff2",
  "media/island-960.avif",
  "media/cave-wide-1440.webp",
  "og/home.jpg",
  "brand-512.png",
  "_headers",
  "_routes.json",
  "_worker.js",
]) {
  assert.ok(existsSync(join(dist, file)), `missing ${file}`);
}

console.log(
  JSON.stringify(
    {
      routes: routes.length,
      public: routes.filter((route) => !route.private).length,
      titles: titles.size,
      canonicals: canonicals.size,
      sitemap: locs.size,
    },
    null,
    2,
  ),
);
