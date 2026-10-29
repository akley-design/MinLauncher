import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const dist = join(root, "dist");
const ssrEntry = join(root, "dist-ssr", "entry-server.js");
const template = await readFile(join(dist, "index.html"), "utf8");

const { renderRoute, routes, renderSitemap, renderFeed } = await import(
  pathToFileURL(ssrEntry).href
);

function injectHead(html, head) {
  const start = html.indexOf("<!--seo:start-->");
  const end = html.indexOf("<!--seo:end-->");
  if (start < 0 || end < 0 || end < start) {
    throw new Error("index.html is missing the SEO markers");
  }
  return (
    html.slice(0, start + "<!--seo:start-->".length) +
    "\n    " +
    head +
    "\n    " +
    html.slice(end)
  );
}

function injectBody(html, body) {
  if (html.includes("<!--app-html-->")) {
    return html.replace("<!--app-html-->", () => body);
  }
  return html.replace(
    '<div id="root"></div>',
    () => `<div id="root">${body}</div>`,
  );
}

function assemble(route) {
  let html = template;
  html = injectHead(html, route.head);
  html = injectBody(html, route.html);
  if (!route.private) {
    html = html.replace(/<html([^>]*)>/, (_match, attributes) => {
      if (String(attributes).includes("data-prerendered"))
        return `<html${attributes}>`;
      return `<html${attributes} data-prerendered="public">`;
    });
  }
  return html;
}

for (const route of routes) {
  const rendered = renderRoute(route.path);
  const target =
    route.path === "/"
      ? join(dist, "index.html")
      : join(dist, route.path.slice(1), "index.html");
  await mkdir(dirname(target), { recursive: true });
  await writeFile(target, assemble(rendered));
  console.log(`prerendered ${route.path}${route.private ? " (shell)" : ""}`);
}

const notFound = renderRoute("/404");
await writeFile(join(dist, "404.html"), assemble(notFound));
await writeFile(join(dist, "sitemap.xml"), renderSitemap());
await writeFile(join(dist, "feed.xml"), renderFeed());
console.log(`prerendered 404, sitemap.xml, feed.xml (${routes.length} routes)`);
