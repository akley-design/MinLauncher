const GITHUB_MANIFEST_URL = "https://github.com/akley-design/MinLauncher/releases/latest/download/latest.json";
const MANIFEST_URL = import.meta.env.DEV ? GITHUB_MANIFEST_URL : "/api/latest.json";

export async function getWindowsDownload(signal?: AbortSignal): Promise<string> {
  const response = await fetch(MANIFEST_URL, { signal });
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  const manifest: unknown = await response.json();
  if (!manifest || typeof manifest !== "object" || !("platforms" in manifest)) {
    throw new Error("Invalid release manifest");
  }
  const platforms = manifest.platforms;
  if (!platforms || typeof platforms !== "object" || !("windows-x86_64-nsis" in platforms)) {
    throw new Error("Windows installer is missing");
  }
  const asset = platforms["windows-x86_64-nsis"];
  if (!asset || typeof asset !== "object" || !("url" in asset) || typeof asset.url !== "string") {
    throw new Error("Windows installer URL is missing");
  }
  const url = new URL(asset.url);
  if (url.protocol !== "https:" || url.hostname !== "github.com") {
    throw new Error("Unexpected Windows installer host");
  }
  return url.toString();
}

