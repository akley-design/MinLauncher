import { articles } from "./news";
import { mediaPreload, siteMedia } from "./media";
import {
  OG_HEIGHT,
  OG_WIDTH,
  PUBLIC_ROUTES,
  SITE_ORIGIN,
  isPrivateRoute,
  resolvePage,
  type ResolvedPage,
} from "./page-meta";

const SOFTWARE_VERSION = "0.1.1-beta";
const FALLBACK_IMAGE = `${SITE_ORIGIN}/social-card.png`;

function absolute(path: string) {
  return path.startsWith("http") ? path : `${SITE_ORIGIN}${path}`;
}

function escapeHtml(value: string) {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function metaTag(attribute: "name" | "property", key: string, content: string) {
  return `<meta ${attribute}="${key}" content="${escapeHtml(content)}" />`;
}

function preloadFor(resolved: ResolvedPage) {
  if (
    isPrivateRoute(resolved.meta.canonical ?? "") ||
    resolved.page === "not-found"
  )
    return null;
  const key =
    resolved.page === "article"
      ? `article-${resolved.article!.scene}`
      : resolved.page;
  return mediaPreload[key] ?? null;
}

export function structuredData(resolved: ResolvedPage) {
  const graph: Array<Record<string, unknown>> = [];
  const meta = resolved.meta;
  const organization = {
    "@type": "Organization",
    "@id": `${SITE_ORIGIN}/#organization`,
    name: "MinLauncher",
    url: SITE_ORIGIN,
    logo: {
      "@type": "ImageObject",
      url: absolute(siteMedia.brand),
      width: 512,
      height: 512,
    },
    sameAs: ["https://github.com/akley-design/MinLauncher"],
  };
  const website = {
    "@type": "WebSite",
    "@id": `${SITE_ORIGIN}/#website`,
    url: SITE_ORIGIN,
    name: "MinLauncher",
    description: meta.description,
    inLanguage: "ru",
    publisher: { "@id": `${SITE_ORIGIN}/#organization` },
  };
  const application = {
    "@type": "SoftwareApplication",
    "@id": `${SITE_ORIGIN}/#application`,
    name: "MinLauncher",
    url: meta.canonical ?? SITE_ORIGIN,
    description: meta.description,
    applicationCategory: "GameApplication",
    operatingSystem: "Windows 10, Windows 11",
    softwareVersion: SOFTWARE_VERSION,
    downloadUrl: `${SITE_ORIGIN}/download`,
    image: meta.ogImage ?? FALLBACK_IMAGE,
    offers: {
      "@type": "Offer",
      price: "0",
      priceCurrency: "RUB",
      availability: "https://schema.org/InStock",
    },
  };

  if (resolved.page === "home") {
    graph.push(organization, website, application);
  } else if (resolved.page === "download") {
    graph.push(application);
  }

  if (
    resolved.page === "features" ||
    resolved.page === "download" ||
    resolved.page === "news" ||
    resolved.page === "article"
  ) {
    const items = [
      {
        "@type": "ListItem",
        position: 1,
        name: "Главная",
        item: `${SITE_ORIGIN}/`,
      },
    ];
    if (resolved.page === "features") {
      items.push({
        "@type": "ListItem",
        position: 2,
        name: "Возможности",
        item: `${SITE_ORIGIN}/features`,
      });
    } else if (resolved.page === "download") {
      items.push({
        "@type": "ListItem",
        position: 2,
        name: "Скачать",
        item: `${SITE_ORIGIN}/download`,
      });
    } else {
      items.push({
        "@type": "ListItem",
        position: 2,
        name: "Журнал",
        item: `${SITE_ORIGIN}/news`,
      });
      if (resolved.page === "article" && resolved.article) {
        items.push({
          "@type": "ListItem",
          position: 3,
          name: resolved.article.title,
          item: meta.canonical ?? `${SITE_ORIGIN}/news`,
        });
      }
    }
    graph.push({
      "@type": "BreadcrumbList",
      itemListElement: items,
    });
  }

  if (resolved.page === "article" && resolved.article) {
    const article = resolved.article;
    graph.push({
      "@type": "Article",
      headline: article.title,
      description: article.excerpt,
      datePublished: article.publishedAt,
      dateModified: article.updatedAt,
      author: {
        "@type": "Organization",
        name: article.author,
      },
      publisher: {
        "@type": "Organization",
        name: "MinLauncher",
        logo: {
          "@type": "ImageObject",
          url: absolute(siteMedia.brand),
          width: 512,
          height: 512,
        },
      },
      image: meta.ogImage ?? FALLBACK_IMAGE,
      mainEntityOfPage: {
        "@type": "WebPage",
        "@id": meta.canonical ?? SITE_ORIGIN,
      },
      inLanguage: "ru",
    });
  }

  return graph.length
    ? {
        "@context": "https://schema.org",
        "@graph": graph,
      }
    : null;
}

export function renderHead(pathname: string) {
  const resolved = resolvePage(pathname);
  const meta = resolved.meta;
  const ogImage = meta.ogImage ?? FALLBACK_IMAGE;
  const imageType = ogImage.endsWith(".png") ? "image/png" : "image/jpeg";
  const robots = meta.noindex
    ? "noindex, nofollow"
    : "index, follow, max-image-preview:large";
  const preload = preloadFor(resolved);
  const data = structuredData(resolved);
  const lines = [
    `<title>${escapeHtml(meta.title)}</title>`,
    metaTag("name", "description", meta.description),
    metaTag("name", "robots", robots),
    meta.canonical
      ? `<link rel="canonical" href="${escapeHtml(meta.canonical)}" />`
      : "",
    `<link rel="alternate" type="application/rss+xml" title="Журнал MinLauncher" href="${SITE_ORIGIN}/feed.xml" />`,
    metaTag("property", "og:title", meta.title),
    metaTag("property", "og:description", meta.description),
    metaTag("property", "og:site_name", "MinLauncher"),
    metaTag("property", "og:locale", "ru_RU"),
    metaTag("property", "og:type", meta.ogType ?? "website"),
    metaTag("property", "og:url", meta.canonical ?? SITE_ORIGIN),
    metaTag("property", "og:image", ogImage),
    metaTag("property", "og:image:type", imageType),
    metaTag("property", "og:image:width", String(OG_WIDTH)),
    metaTag("property", "og:image:height", String(OG_HEIGHT)),
    metaTag("property", "og:image:alt", meta.ogImageAlt ?? "MinLauncher"),
    metaTag("name", "twitter:card", "summary_large_image"),
    metaTag("name", "twitter:title", meta.title),
    metaTag("name", "twitter:description", meta.description),
    metaTag("name", "twitter:image", ogImage),
    metaTag("name", "twitter:image:alt", meta.ogImageAlt ?? "MinLauncher"),
    meta.articlePublishedAt
      ? metaTag("property", "article:published_time", meta.articlePublishedAt)
      : "",
    meta.articlePublishedAt
      ? metaTag("name", "author", resolved.article?.author ?? "MinLauncher")
      : "",
    preload
      ? `<link rel="preload" as="image" type="${preload.type}" href="${preload.href}" imagesrcset="${escapeHtml(preload.srcSet)}" imagesizes="${escapeHtml(preload.sizes)}" fetchpriority="high" />`
      : "",
    data
      ? `<script type="application/ld+json">${JSON.stringify(data).replaceAll("<", "\\u003c")}</script>`
      : "",
  ];
  return lines.filter(Boolean).join("\n    ");
}

export function renderSitemap() {
  const today = new Date().toISOString().slice(0, 10);
  const urls = PUBLIC_ROUTES.map((path) => {
    const resolved = resolvePage(path);
    const loc = resolved.meta.canonical ?? absolute(path);
    const lastmod = resolved.article?.updatedAt.slice(0, 10) ?? today;
    return `  <url><loc>${escapeHtml(loc)}</loc><lastmod>${lastmod}</lastmod></url>`;
  });
  return `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls.join("\n")}\n</urlset>\n`;
}

export function renderFeed() {
  const items = articles.map((article) => {
    const link = `${SITE_ORIGIN}/news/${article.slug}`;
    return [
      "  <item>",
      `    <title>${escapeHtml(article.title)}</title>`,
      `    <link>${link}</link>`,
      `    <guid isPermaLink="true">${link}</guid>`,
      `    <description>${escapeHtml(article.excerpt)}</description>`,
      `    <author>${escapeHtml(article.author)}</author>`,
      `    <pubDate>${new Date(article.publishedAt).toUTCString()}</pubDate>`,
      "  </item>",
    ].join("\n");
  });
  return `<?xml version="1.0" encoding="UTF-8"?>\n<rss version="2.0">\n<channel>\n  <title>Журнал MinLauncher</title>\n  <link>${SITE_ORIGIN}/news</link>\n  <description>Заметки о релизах, производительности и устройстве MinLauncher.</description>\n  <language>ru</language>\n  <lastBuildDate>${new Date().toUTCString()}</lastBuildDate>\n${items.join("\n")}\n</channel>\n</rss>\n`;
}
