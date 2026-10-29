import { useEffect } from "react";
import { articles, type NewsArticle, type SceneArt } from "./news";
import { siteMedia } from "./media";

export type PageId =
  | "home"
  | "features"
  | "download"
  | "news"
  | "article"
  | "account"
  | "settings"
  | "reset"
  | "callback"
  | "not-found";

export type PageAccent =
  "lime" | "copper" | "violet" | "cream" | "mint" | "parchment";

export interface PageMetaInfo {
  title: string;
  description: string;
  canonical?: string;
  noindex?: boolean;
  ogType?: "website" | "article";
  ogImage?: string;
  ogImageAlt?: string;
  articlePublishedAt?: string;
}

export interface ResolvedPage {
  page: PageId;
  accent: PageAccent;
  meta: PageMetaInfo;
  article?: NewsArticle;
}

export const SITE_ORIGIN = "https://minlauncher.pages.dev";
const SHARE_IMAGE = `${SITE_ORIGIN}/social-card.png`;
export const OG_WIDTH = 1200;
export const OG_HEIGHT = 630;

const ACCENTS: Record<SceneArt, PageAccent> = {
  island: "lime",
  cave: "copper",
  portal: "violet",
};

function absolute(path: string) {
  return path.startsWith("http") ? path : `${SITE_ORIGIN}${path}`;
}

export const PUBLIC_ROUTES: string[] = [
  "/",
  "/features",
  "/download",
  "/news",
  ...articles.map((article) => `/news/${article.slug}`),
];

export const PRIVATE_ROUTES: string[] = [
  "/account",
  "/account/settings",
  "/auth/reset-password",
  "/auth/callback",
];

export const STATIC_ROUTES: string[] = [...PUBLIC_ROUTES, ...PRIVATE_ROUTES];

const PRIVATE_SET = new Set(PRIVATE_ROUTES);

export function normalize(pathname: string) {
  let value = pathname;
  try {
    value = decodeURIComponent(pathname);
  } catch {
    value = pathname;
  }
  return value.replace(/\/+$/, "") || "/";
}

export function isPrivateRoute(pathname: string) {
  return PRIVATE_SET.has(normalize(pathname));
}

export function resolvePage(pathname: string): ResolvedPage {
  const path = normalize(pathname);
  if (path === "/") {
    return {
      page: "home",
      accent: "lime",
      meta: {
        title: "MinLauncher — лаунчер Minecraft для Windows",
        description:
          "MinLauncher — быстрый лаунчер Minecraft Java для Windows: версии, изолированные сборки, настройка Java и памяти, замеры запуска. Скачать бесплатно.",
        canonical: `${SITE_ORIGIN}/`,
        ogType: "website",
        ogImage: absolute(siteMedia.og.home),
        ogImageAlt: "MinLauncher — лаунчер Minecraft для Windows",
      },
    };
  }
  if (path === "/features") {
    return {
      page: "features",
      accent: "copper",
      meta: {
        title: "Возможности MinLauncher — профили и настройки Minecraft",
        description:
          "Игровые профили, отдельные версии Minecraft, настройка Java и памяти, повторное использование файлов и замеры запуска в MinLauncher.",
        canonical: `${SITE_ORIGIN}/features`,
        ogType: "website",
        ogImage: absolute(siteMedia.og.features),
        ogImageAlt: "Пещера в стиле Minecraft — возможности MinLauncher",
      },
    };
  }
  if (path === "/download") {
    return {
      page: "download",
      accent: "violet",
      meta: {
        title: "Скачать MinLauncher для Windows — лаунчер Minecraft",
        description:
          "Скачать MinLauncher для Windows x64: версия 0.1.1-beta, дата публикации и установщик лаунчера Minecraft. Бесплатно.",
        canonical: `${SITE_ORIGIN}/download`,
        ogType: "website",
        ogImage: absolute(siteMedia.og.download),
        ogImageAlt: "Портал в стиле Minecraft — скачать MinLauncher",
      },
    };
  }
  if (path === "/news") {
    return {
      page: "news",
      accent: "cream",
      meta: {
        title: "Журнал MinLauncher — релизы и производительность Minecraft",
        description:
          "Заметки о релизах MinLauncher, производительности запуска Minecraft и устройстве лаунчера.",
        canonical: `${SITE_ORIGIN}/news`,
        ogType: "website",
        ogImage: absolute(siteMedia.og.news),
        ogImageAlt: "Журнал MinLauncher о Minecraft и запуске игры",
      },
    };
  }
  if (path.startsWith("/news/")) {
    const slug = path.slice("/news/".length);
    const article = articles.find((item) => item.slug === slug);
    if (article) {
      return {
        page: "article",
        accent: ACCENTS[article.scene],
        meta: {
          title: `${article.title} — журнал MinLauncher`,
          description: article.excerpt,
          canonical: `${SITE_ORIGIN}/news/${article.slug}`,
          ogType: "article",
          ogImage: absolute(siteMedia.og.article[article.scene]),
          ogImageAlt: `Иллюстрация к статье «${article.title}»`,
          articlePublishedAt: article.publishedAt,
        },
        article,
      };
    }
  }
  if (path === "/account") {
    return {
      page: "account",
      accent: "mint",
      meta: {
        title: "Вход в аккаунт — MinLauncher",
        description:
          "Вход и регистрация MinLauncher по нику Minecraft, почте и паролю.",
        noindex: true,
        ogImage: SHARE_IMAGE,
        ogImageAlt: "MinLauncher",
      },
    };
  }
  if (path === "/account/settings") {
    return {
      page: "settings",
      accent: "parchment",
      meta: {
        title: "Настройки профиля — MinLauncher",
        description:
          "Ник, канал обновлений и привязка Microsoft / Minecraft в профиле MinLauncher.",
        noindex: true,
        ogImage: SHARE_IMAGE,
        ogImageAlt: "MinLauncher",
      },
    };
  }
  if (path === "/auth/reset-password") {
    return {
      page: "reset",
      accent: "copper",
      meta: {
        title: "Восстановление пароля — MinLauncher",
        description:
          "Восстановление доступа к аккаунту MinLauncher по электронной почте.",
        noindex: true,
        ogImage: SHARE_IMAGE,
        ogImageAlt: "MinLauncher",
      },
    };
  }
  if (path === "/auth/callback") {
    return {
      page: "callback",
      accent: "violet",
      meta: {
        title: "Вход в MinLauncher",
        description: "Завершаем вход и передаём сессию в лаунчер.",
        noindex: true,
        ogImage: SHARE_IMAGE,
        ogImageAlt: "MinLauncher",
      },
    };
  }
  return {
    page: "not-found",
    accent: "copper",
    meta: {
      title: "Страница не найдена — MinLauncher",
      description:
        "Такой страницы нет. Вернитесь на главную MinLauncher или к загрузке.",
      noindex: true,
      ogImage: SHARE_IMAGE,
      ogImageAlt: "MinLauncher",
    },
  };
}

function upsertMeta(
  attribute: "name" | "property",
  key: string,
  content: string | null,
) {
  const selector = `meta[${attribute}="${key}"]`;
  const existing = document.head.querySelector<HTMLMetaElement>(selector);
  if (content === null) {
    existing?.remove();
    return;
  }
  if (existing) {
    existing.setAttribute("content", content);
    return;
  }
  const tag = document.createElement("meta");
  tag.setAttribute(attribute, key);
  tag.setAttribute("content", content);
  document.head.append(tag);
}

function upsertCanonical(href: string | null) {
  const existing = document.head.querySelector<HTMLLinkElement>(
    'link[rel="canonical"]',
  );
  if (href === null) {
    existing?.remove();
    return;
  }
  if (existing) {
    existing.setAttribute("href", href);
    return;
  }
  const link = document.createElement("link");
  link.rel = "canonical";
  link.href = href;
  document.head.append(link);
}

export function PageMeta({ meta }: { meta: PageMetaInfo }) {
  useEffect(() => {
    const ogImage = meta.ogImage ?? SHARE_IMAGE;
    document.title = meta.title;
    upsertMeta("name", "description", meta.description);
    upsertMeta(
      "name",
      "robots",
      meta.noindex
        ? "noindex, nofollow"
        : "index, follow, max-image-preview:large",
    );
    upsertCanonical(meta.canonical ?? null);
    upsertMeta("property", "og:title", meta.title);
    upsertMeta("property", "og:description", meta.description);
    upsertMeta("property", "og:site_name", "MinLauncher");
    upsertMeta("property", "og:locale", "ru_RU");
    upsertMeta("property", "og:type", meta.ogType ?? "website");
    upsertMeta("property", "og:url", meta.canonical ?? SITE_ORIGIN);
    upsertMeta("property", "og:image", ogImage);
    upsertMeta("property", "og:image:width", String(OG_WIDTH));
    upsertMeta("property", "og:image:height", String(OG_HEIGHT));
    upsertMeta("property", "og:image:alt", meta.ogImageAlt ?? "MinLauncher");
    upsertMeta(
      "property",
      "article:published_time",
      meta.articlePublishedAt ?? null,
    );
    upsertMeta("name", "twitter:card", "summary_large_image");
    upsertMeta("name", "twitter:title", meta.title);
    upsertMeta("name", "twitter:description", meta.description);
    upsertMeta("name", "twitter:image", ogImage);
    upsertMeta("name", "twitter:image:alt", meta.ogImageAlt ?? "MinLauncher");
  }, [meta]);
  return null;
}
