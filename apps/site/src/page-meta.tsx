import { useEffect } from "react";
import { articles, type NewsArticle, type SceneArt } from "./news";

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
}

export interface ResolvedPage {
  page: PageId;
  accent: PageAccent;
  meta: PageMetaInfo;
  article?: NewsArticle;
}

export const SITE_ORIGIN = "https://minlauncher.pages.dev";
const SHARE_IMAGE = `${SITE_ORIGIN}/social-card.png`;

const ACCENTS: Record<SceneArt, PageAccent> = {
  island: "lime",
  cave: "copper",
  portal: "violet",
};

function normalize(pathname: string) {
  let value = pathname;
  try {
    value = decodeURIComponent(pathname);
  } catch {
    value = pathname;
  }
  return value.replace(/\/+$/, "") || "/";
}

export function resolvePage(pathname: string): ResolvedPage {
  const path = normalize(pathname);
  if (path === "/") {
    return {
      page: "home",
      accent: "lime",
      meta: {
        title: "MinLauncher · Твой Minecraft",
        description:
          "Версии Minecraft, отдельные сборки, настройки Java и результаты замеров в MinLauncher для Windows.",
        canonical: `${SITE_ORIGIN}/`,
        ogType: "website",
      },
    };
  }
  if (path === "/features") {
    return {
      page: "features",
      accent: "copper",
      meta: {
        title: "Возможности · MinLauncher",
        description:
          "Игровые профили, Java и память, повторное использование файлов и замеры запуска.",
        canonical: `${SITE_ORIGIN}/features`,
        ogType: "website",
      },
    };
  }
  if (path === "/download") {
    return {
      page: "download",
      accent: "violet",
      meta: {
        title: "Скачать MinLauncher для Windows",
        description:
          "Релиз MinLauncher для Windows x64: версия, дата публикации и установщик.",
        canonical: `${SITE_ORIGIN}/download`,
        ogType: "website",
      },
    };
  }
  if (path === "/news") {
    return {
      page: "news",
      accent: "cream",
      meta: {
        title: "Журнал · MinLauncher",
        description:
          "Заметки о релизах, производительности и устройстве MinLauncher.",
        canonical: `${SITE_ORIGIN}/news`,
        ogType: "website",
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
          title: `${article.title} · Журнал MinLauncher`,
          description: article.excerpt,
          canonical: `${SITE_ORIGIN}/news/${article.slug}`,
          ogType: "article",
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
        title: "Вход в аккаунт · MinLauncher",
        description:
          "Вход и регистрация MinLauncher по нику Minecraft, почте и паролю.",
        noindex: true,
      },
    };
  }
  if (path === "/account/settings") {
    return {
      page: "settings",
      accent: "parchment",
      meta: {
        title: "Настройки профиля · MinLauncher",
        description:
          "Ник, канал обновлений и привязка Microsoft / Minecraft в профиле MinLauncher.",
        noindex: true,
      },
    };
  }
  if (path === "/auth/reset-password") {
    return {
      page: "reset",
      accent: "copper",
      meta: {
        title: "Восстановление пароля · MinLauncher",
        description:
          "Восстановление доступа к аккаунту MinLauncher по электронной почте.",
        noindex: true,
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
      },
    };
  }
  return {
    page: "not-found",
    accent: "copper",
    meta: {
      title: "Страница не найдена · MinLauncher",
      description:
        "Такой страницы нет. Вернитесь на главную MinLauncher или к загрузке.",
      noindex: true,
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
    document.title = meta.title;
    upsertMeta("name", "description", meta.description);
    upsertMeta("name", "robots", meta.noindex ? "noindex, nofollow" : null);
    upsertCanonical(meta.canonical ?? null);
    upsertMeta("property", "og:title", meta.title);
    upsertMeta("property", "og:description", meta.description);
    upsertMeta("property", "og:site_name", "MinLauncher");
    upsertMeta("property", "og:locale", "ru_RU");
    upsertMeta("property", "og:type", meta.ogType ?? "website");
    upsertMeta("property", "og:url", meta.canonical ?? SITE_ORIGIN);
    upsertMeta("property", "og:image", SHARE_IMAGE);
    upsertMeta("name", "twitter:card", "summary_large_image");
    upsertMeta("name", "twitter:title", meta.title);
    upsertMeta("name", "twitter:description", meta.description);
    upsertMeta("name", "twitter:image", SHARE_IMAGE);
  }, [meta]);
  return null;
}
