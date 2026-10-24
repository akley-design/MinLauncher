import { afterEach, describe, expect, it, vi } from "vitest";
import { getWindowsDownload } from "./downloads";

describe("getWindowsDownload", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("returns the signed Windows installer URL from the release manifest", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(JSON.stringify({
      platforms: { "windows-x86_64-nsis": { url: "https://github.com/akley-design/MinLauncher/releases/download/v1/setup.exe", signature: "sig" } },
    }), { status: 200 })));
    await expect(getWindowsDownload()).resolves.toBe("https://github.com/akley-design/MinLauncher/releases/download/v1/setup.exe");
  });

  it("rejects download URLs outside GitHub HTTPS releases", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(JSON.stringify({
      platforms: { "windows-x86_64-nsis": { url: "https://example.com/setup.exe" } },
    }), { status: 200 })));
    await expect(getWindowsDownload()).rejects.toThrow("Unexpected Windows installer host");
  });

  it("rejects incomplete or unavailable manifests", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("{}", { status: 200 })));
    await expect(getWindowsDownload()).rejects.toThrow("Invalid release manifest");
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("", { status: 503 })));
    await expect(getWindowsDownload()).rejects.toThrow("HTTP 503");
  });
});
