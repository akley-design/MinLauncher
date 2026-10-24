import { createServer } from "node:http";
import { readFile, stat } from "node:fs/promises";
import { extname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const types = {
  ".css": "text/css; charset=utf-8",
  ".html": "text/html; charset=utf-8",
  ".ico": "image/x-icon",
  ".jpg": "image/jpeg",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".png": "image/png",
  ".svg": "image/svg+xml",
  ".webp": "image/webp",
};
const releaseManifest = "https://github.com/akley-design/MinLauncher/releases/latest/download/latest.json";

function safePath(pathname, root) {
  const decoded = decodeURIComponent(pathname);
  const file = resolve(root, `.${decoded}`);
  const rel = relative(root, file);
  if (rel === ".." || rel.startsWith(`..${sep}`) || resolve(file) === root && decoded !== "/") return null;
  return file;
}

export function createSiteServer(siteRoot = fileURLToPath(new URL("./dist/", import.meta.url)), fetchRelease = fetch) {
  const root = resolve(siteRoot);
  return createServer(async (req, res) => {
    res.setHeader("X-Content-Type-Options", "nosniff");
    res.setHeader("X-Frame-Options", "DENY");
    res.setHeader("Referrer-Policy", "strict-origin-when-cross-origin");
    res.setHeader("Content-Security-Policy", "default-src 'self'; img-src 'self' data:; style-src 'self'; font-src 'self' data:; script-src 'self'; connect-src 'self' https: wss:; base-uri 'self'; form-action 'self'; frame-ancestors 'none'");

    let pathname;
    try {
      pathname = new URL(req.url ?? "/", "http://localhost").pathname;
    } catch {
      res.writeHead(400).end("Bad request");
      return;
    }

    if (req.method !== "GET" && req.method !== "HEAD") {
      res.writeHead(405, { Allow: "GET, HEAD" }).end("Method not allowed");
      return;
    }
    if (pathname === "/api/latest.json") {
      try {
        const upstream = await fetchRelease(releaseManifest, { signal: AbortSignal.timeout(6000) });
        if (!upstream.ok) throw new Error(`GitHub returned ${upstream.status}`);
        const body = await upstream.text();
        res.setHeader("Content-Type", "application/json; charset=utf-8");
        res.setHeader("Cache-Control", "public, max-age=300");
        res.writeHead(200).end(req.method === "HEAD" ? undefined : body);
      } catch {
        res.writeHead(502, { "Cache-Control": "no-store" }).end("Release manifest unavailable");
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

    try {
      if ((await stat(file)).isDirectory()) file = join(file, "index.html");
      var body = await readFile(file);
    } catch {
      if (req.headers.accept?.includes("text/html")) {
        try {
          file = join(root, "index.html");
          body = await readFile(file);
        } catch {
          res.writeHead(503).end("Site is not built yet");
          return;
        }
      } else {
        res.writeHead(404).end("Not found");
        return;
      }
    }

    const immutableAsset = file.includes(`${sep}assets${sep}`);
    const contentType = types[extname(file).toLowerCase()] ?? "application/octet-stream";
    const compressible = contentType.startsWith("text/") || contentType.startsWith("application/");
    let responseBody = body;
    res.setHeader("Content-Type", contentType);
    res.setHeader("Cache-Control", immutableAsset ? "public, max-age=31536000, immutable" : "no-cache");
    res.setHeader("Vary", "Accept-Encoding");
    if (compressible && body.length > 256 && /\bgzip\b/i.test(req.headers["accept-encoding"] ?? "")) {
      responseBody = gzipSync(body);
      res.setHeader("Content-Encoding", "gzip");
    }
    res.writeHead(200).end(req.method === "HEAD" ? undefined : responseBody);
  });
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const port = Number(process.env.PORT) || 3000;
  createSiteServer().listen(port, "0.0.0.0", () => console.log(`MinLauncher site listening on ${port}`));
}
