/** Pure helpers shared by the release publisher and its tests. */

export function nextPatchVersion(version) {
  const match = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.exec(version);
  if (!match) throw new Error(`Expected a release version in X.Y.Z form, got: ${version}`);
  const patch = Number(match[3]);
  if (!Number.isSafeInteger(patch + 1)) throw new Error(`Patch version is too large: ${version}`);
  return `${match[1]}.${match[2]}.${patch + 1}`;
}

export function isSensitivePath(path) {
  const normalized = path.replaceAll("\\", "/").toLowerCase();
  const name = normalized.split("/").at(-1) ?? "";
  if (name === ".env.example") return false;
  return (
    name === ".env" ||
    name.startsWith(".env.") ||
    name.endsWith(".key") ||
    name.endsWith(".pem") ||
    name === "credentials.json" ||
    name === "secrets.json"
  );
}
