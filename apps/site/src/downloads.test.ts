// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { getWindowsRelease, ReleaseUnavailable } from "./downloads";

describe("release details and availability", () => {
  afterEach(() => vi.unstubAllGlobals());
  const asset = {
    url: "https://github.com/akley-design/MinLauncher/releases/download/v1/setup.exe",
  };
  it("reads version and publication date from the actual manifest", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(
          JSON.stringify({
            version: "1.2.3",
            pub_date: "2026-10-20T12:00:00Z",
            platforms: { "windows-x86_64-nsis": asset },
          }),
        ),
      ),
    );
    await expect(getWindowsRelease()).resolves.toEqual({
      url: asset.url,
      version: "1.2.3",
      publishedAt: "2026-10-20T12:00:00Z",
      beta: false,
    });
  });
  it("distinguishes an unpublished release from a failed request", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(new Response("", { status: 404 })),
    );
    await expect(getWindowsRelease()).rejects.toBeInstanceOf(
      ReleaseUnavailable,
    );
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(new Response("", { status: 502 })),
    );
    await expect(getWindowsRelease()).rejects.not.toBeInstanceOf(
      ReleaseUnavailable,
    );
  });
  it("omits missing metadata and invalid dates", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(
          JSON.stringify({
            pub_date: "not a date",
            platforms: { "windows-x86_64-nsis": asset },
          }),
        ),
      ),
    );
    await expect(getWindowsRelease()).resolves.toEqual({
      url: asset.url,
      version: undefined,
      publishedAt: undefined,
      beta: false,
    });
  });
  it("rejects a manifest without platforms", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(new Response("{}", { status: 200 })),
    );
    await expect(getWindowsRelease()).rejects.toThrow(
      "Invalid release manifest",
    );
  });

  it("rejects URLs outside this project's release assets", async () => {
    for (const url of [
      "https://github.com/another/repo/releases/download/v1/setup.exe",
      "https://github.com/akley-design/MinLauncher",
      "https://user:password@github.com/akley-design/MinLauncher/releases/download/v1/setup.exe",
    ]) {
      vi.stubGlobal(
        "fetch",
        vi
          .fn()
          .mockResolvedValue(
            new Response(
              JSON.stringify({ platforms: { "windows-x86_64-nsis": { url } } }),
            ),
          ),
      );
      await expect(getWindowsRelease()).rejects.toThrow(
        "Unexpected Windows installer host",
      );
    }
  });
});
