const GITHUB_MANIFEST_URL =
  "https://github.com/akley-design/MinLauncher/releases/latest/download/latest.json";
const MANIFEST_URL = import.meta.env.DEV
  ? GITHUB_MANIFEST_URL
  : "/api/latest.json";

export interface WindowsRelease {
  url: string;
  version?: string;
  publishedAt?: string;
  beta?: boolean;
}

export interface ReleaseClock {
  now: number;
  releaseAt: number;
}

export class ReleaseUnavailable extends Error {}

export async function getReleaseClock(
  signal?: AbortSignal,
): Promise<ReleaseClock> {
  const response = await fetch("/api/clock.json", {
    cache: "no-store",
    signal: signal
      ? AbortSignal.any([signal, AbortSignal.timeout(10000)])
      : AbortSignal.timeout(10000),
  });
  if (!response.ok) throw new Error(`Clock HTTP ${response.status}`);
  const value: unknown = await response.json();
  if (
    !value ||
    typeof value !== "object" ||
    !("now" in value) ||
    typeof value.now !== "number" ||
    !("releaseAt" in value) ||
    typeof value.releaseAt !== "number"
  ) {
    throw new Error("Invalid release clock");
  }
  return { now: value.now, releaseAt: value.releaseAt };
}

export async function getWindowsRelease(
  signal?: AbortSignal,
): Promise<WindowsRelease> {
  const response = await fetch(MANIFEST_URL, {
    signal: signal
      ? AbortSignal.any([signal, AbortSignal.timeout(10000)])
      : AbortSignal.timeout(10000),
  });
  if (response.status === 404)
    throw new ReleaseUnavailable("Release is not published");
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  const manifest: unknown = await response.json();
  if (!manifest || typeof manifest !== "object" || !("platforms" in manifest)) {
    throw new Error("Invalid release manifest");
  }
  const platforms = manifest.platforms;
  if (
    !platforms ||
    typeof platforms !== "object" ||
    !("windows-x86_64-nsis" in platforms)
  ) {
    throw new ReleaseUnavailable("Windows installer is missing");
  }
  const asset = platforms["windows-x86_64-nsis"];
  if (
    !asset ||
    typeof asset !== "object" ||
    !("url" in asset) ||
    typeof asset.url !== "string"
  ) {
    throw new Error("Windows installer URL is missing");
  }
  const url = new URL(asset.url, window.location.origin);
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    url.port ||
    !(
      (url.hostname === "github.com" &&
        url.pathname.startsWith(
          "/akley-design/MinLauncher/releases/download/",
        )) ||
      (url.origin === window.location.origin &&
        /^\/MinLauncher_\d+\.\d+\.\d+(?:-[a-z0-9.-]+)?_x64-setup\.exe$/.test(
          url.pathname,
        ))
    )
  ) {
    throw new Error("Unexpected Windows installer host");
  }
  const version =
    "version" in manifest && typeof manifest.version === "string"
      ? manifest.version
      : undefined;
  const publishedAt =
    "pub_date" in manifest &&
    typeof manifest.pub_date === "string" &&
    Number.isFinite(Date.parse(manifest.pub_date))
      ? manifest.pub_date
      : undefined;
  const beta = "beta" in manifest && manifest.beta === true;
  return { url: url.toString(), version, publishedAt, beta };
}
