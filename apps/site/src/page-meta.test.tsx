// @vitest-environment jsdom
import { cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { PageMeta, resolvePage, SITE_ORIGIN } from "./page-meta";

function clearHead() {
  document.head
    .querySelectorAll(
      'meta[property^="og:"], meta[name^="twitter:"], meta[name="description"], meta[name="robots"], link[rel="canonical"]',
    )
    .forEach((element) => element.remove());
  document.title = "";
}

beforeEach(clearHead);
afterEach(cleanup);

describe("site page identity", () => {
  it("gives every public route a unique title and the right canonical", () => {
    const paths = [
      "/",
      "/features",
      "/download",
      "/news",
      "/news/beta-0-1-1",
      "/news/speed-without-magic",
    ];
    const pages = paths.map((path) => resolvePage(path));
    expect(new Set(pages.map((page) => page.meta.title)).size).toBe(
      paths.length,
    );
    expect(pages[0]!.meta.canonical).toBe(`${SITE_ORIGIN}/`);
    expect(pages[1]!.meta.canonical).toBe(`${SITE_ORIGIN}/features`);
    expect(pages[2]!.meta.canonical).toBe(`${SITE_ORIGIN}/download`);
    expect(pages[3]!.meta.canonical).toBe(`${SITE_ORIGIN}/news`);
    expect(pages[4]!.meta.canonical).toBe(`${SITE_ORIGIN}/news/beta-0-1-1`);
    expect(pages[4]!.meta.ogType).toBe("article");
    for (const page of pages) expect(page.meta.noindex).toBeFalsy();
  });

  it("marks account, auth and 404 pages as private", () => {
    for (const path of [
      "/account",
      "/account/settings",
      "/auth/reset-password",
      "/auth/callback",
      "/missing-page",
      "/news/not-a-real-article",
    ]) {
      const page = resolvePage(path);
      expect(page.meta.noindex).toBe(true);
      expect(page.meta.canonical).toBeUndefined();
      expect(page.meta.title.length).toBeGreaterThan(0);
    }
    expect(resolvePage("/news/not-a-real-article").page).toBe("not-found");
  });

  it("upserts metadata without duplicating tags", () => {
    const publicPage = resolvePage("/features");
    const privatePage = resolvePage("/account");
    const view = render(<PageMeta meta={publicPage.meta} />);
    expect(document.title).toBe(publicPage.meta.title);
    expect(document.querySelectorAll('meta[name="description"]')).toHaveLength(
      1,
    );
    expect(document.querySelectorAll('meta[property="og:title"]')).toHaveLength(
      1,
    );
    expect(
      document.querySelector<HTMLLinkElement>('link[rel="canonical"]')?.href,
    ).toBe(`${SITE_ORIGIN}/features`);
    expect(
      document.querySelector<HTMLMetaElement>('meta[name="robots"]')?.content,
    ).toBe("index, follow, max-image-preview:large");

    view.rerender(<PageMeta meta={privatePage.meta} />);
    expect(document.title).toBe(privatePage.meta.title);
    expect(document.querySelector('link[rel="canonical"]')).toBeNull();
    expect(
      document.querySelector<HTMLMetaElement>('meta[name="robots"]')?.content,
    ).toBe("noindex, nofollow");
    expect(document.querySelectorAll('meta[name="description"]')).toHaveLength(
      1,
    );
    expect(document.querySelectorAll('meta[property="og:title"]')).toHaveLength(
      1,
    );

    view.rerender(<PageMeta meta={publicPage.meta} />);
    expect(
      document.querySelector<HTMLMetaElement>('meta[name="robots"]')?.content,
    ).toBe("index, follow, max-image-preview:large");
    expect(document.querySelectorAll('link[rel="canonical"]')).toHaveLength(1);
  });
});
