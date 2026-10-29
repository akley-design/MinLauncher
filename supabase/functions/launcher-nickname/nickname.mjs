export function normalizeNickname(value) {
  if (typeof value !== "string" || !/^[A-Za-z0-9_]{3,16}$/.test(value))
    return null;
  return value.toLowerCase();
}
