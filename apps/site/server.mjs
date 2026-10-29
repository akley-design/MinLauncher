import { createServer } from "node:http";
import { readFile, stat } from "node:fs/promises";
import { extname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const types = {
  ".avif": "image/avif",
  ".css": "text/css; charset=utf-8",
  ".html": "text/html; charset=utf-8",
  ".ico": "image/x-icon",
  ".jpg": "image/jpeg",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".png": "image/png",
  ".svg": "image/svg+xml",
  ".txt": "text/plain; charset=utf-8",
  ".webp": "image/webp",
  ".woff2": "font/woff2",
  ".xml": "application/xml; charset=utf-8",
  ".ttf": "font/ttf",
};
const releaseManifest =
  "https://github.com/akley-design/MinLauncher/releases/latest/download/latest.json";

function safePath(pathname, root) {
  const decoded = decodeURIComponent(pathname);
  const file = resolve(root, `.${decoded}`);
  const rel = relative(root, file);
  if (
    rel === ".." ||
    rel.startsWith(`..${sep}`) ||
    (resolve(file) === root && decoded !== "/")
  )
    return null;
  return file;
}

export function createSiteServer(
  siteRoot = fileURLToPath(new URL("./dist/", import.meta.url)),
  fetchRelease = fetch,
) {
  const root = resolve(siteRoot);
  return createServer(async (req, res) => {
    res.setHeader("X-Content-Type-Options", "nosniff");
    res.setHeader("X-Frame-Options", "DENY");
    res.setHeader("Referrer-Policy", "strict-origin-when-cross-origin");
    res.setHeader(
      "Permissions-Policy",
      "camera=(), microphone=(), geolocation=()",
    );
    res.setHeader(
      "Content-Security-Policy",
      "default-src 'self'; img-src 'self' data:; style-src 'self'; font-src 'self' data:; script-src 'self'; connect-src 'self' https: wss:; base-uri 'self'; form-action 'self'; frame-ancestors 'none'",
    );

    let pathname;
    let search;
    try {
      const url = new URL(req.url ?? "/", "http://localhost");
      pathname = url.pathname;
      search = url.search;
    } catch {
      res.writeHead(400).end("Bad request");
      return;
    }

    if (req.method !== "GET" && req.method !== "HEAD") {
      res.writeHead(405, { Allow: "GET, HEAD" }).end("Method not allowed");
      return;
    }
    if (pathname === "/index.html") {
      res.writeHead(308, { Location: `/${search}` }).end();
      return;
    }
    if (pathname !== "/" && pathname.endsWith("/")) {
      res
        .writeHead(308, {
          Location: `${pathname.replace(/\/+$/, "")}${search}`,
        })
        .end();
      return;
    }
    if (pathname === "/api/clock.json") {
      res.setHeader("Content-Type", "application/json; charset=utf-8");
      res.setHeader("Cache-Control", "no-store, no-cache, must-revalidate");
      res.writeHead(200).end(
        req.method === "HEAD"
          ? undefined
          : JSON.stringify({
              now: Date.now(),
              releaseAt: Date.parse("2026-10-27T22:04:21.000Z"),
            }),
      );
      return;
    }
    if (pathname === "/api/latest.json") {
      try {
        const upstream = await fetchRelease(releaseManifest, {
          method: req.method,
          signal: AbortSignal.timeout(6000),
        });
        if (upstream.status === 404) {
          res
            .writeHead(404, { "Cache-Control": "no-store" })
            .end("Release is not published");
          return;
        }
        if (!upstream.ok) throw new Error(`GitHub returned ${upstream.status}`);
        res.setHeader("Content-Type", "application/json; charset=utf-8");
        res.setHeader("Cache-Control", "public, max-age=300");
        res
          .writeHead(200)
          .end(req.method === "HEAD" ? undefined : await upstream.text());
      } catch {
        res
          .writeHead(502, { "Cache-Control": "no-store" })
          .end("Release manifest unavailable");
      }
      return;
    }

    let file;
    try {
      file = safePath(pathname, root);
    } catch {
      res.writeHead(400).end("Bad request");
      return;
    }
    if (!file) {
      res.writeHead(403).end("Forbidden");
      return;
    }

    let size;
    let body;
    let status = 200;
    const headOnly = req.method === "HEAD";
    try {
      let info = await stat(file);
      if (info.isDirectory()) {
        file = join(file, "index.html");
        info = await stat(file);
      }
      size = info.size;
      if (!headOnly) body = await readFile(file);
    } catch {
      try {
        const candidate = join(file, "index.html");
        const info = await stat(candidate);
        file = candidate;
        size = info.size;
        if (!headOnly) body = await readFile(file);
      } catch {
        if (/\.[a-z0-9]+$/i.test(pathname)) {
          res.writeHead(404).end("Not found");
          return;
        }
        try {
          file = join(root, "404.html");
          size = (await stat(file)).size;
          if (!headOnly) body = await readFile(file);
          status = 404;
        } catch {
          res.writeHead(503).end("Site is not built yet");
          return;
        }
      }
    }

    const immutableAsset = file.includes(`${sep}assets${sep}`);
    const longCache =
      immutableAsset ||
      file.includes(`${sep}fonts${sep}`) ||
      file.includes(`${sep}media${sep}`) ||
      file.includes(`${sep}og${sep}`);
    const contentType =
      types[extname(file).toLowerCase()] ?? "application/octet-stream";
    const compressible =
      contentType.startsWith("text/") || contentType.startsWith("application/");
    res.setHeader("Content-Type", contentType);
    res.setHeader(
      "Cache-Control",
      immutableAsset
        ? "public, max-age=31536000, immutable"
        : longCache
          ? "public, max-age=604800"
          : "no-cache",
    );
    if (status === 404) res.setHeader("X-Robots-Tag", "noindex");
    res.setHeader("Vary", "Accept-Encoding");
    if (
      compressible &&
      size > 256 &&
      /\bgzip\b/i.test(req.headers["accept-encoding"] ?? "")
    ) {
      res.setHeader("Content-Encoding", "gzip");
    }
    if (headOnly) {
      res.writeHead(status).end();
      return;
    }
    const responseBody = res.hasHeader("Content-Encoding")
      ? gzipSync(body)
      : body;
    res.writeHead(status).end(responseBody);
  });
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const port = Number(process.env.PORT) || 3000;
  createSiteServer().listen(port, "0.0.0.0", () =>
    console.log(`MinLauncher site listening on ${port}`),
  );
}
