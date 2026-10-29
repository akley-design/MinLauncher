/** @param {unknown} error */
export function minecraftProviderErrorCategory(error) {
  return error === "access_denied" ? "cancelled" : "provider_error";
}

/** @param {unknown} entitlements */
export function hasMinecraftEntitlement(entitlements) {
  return (
    typeof entitlements === "object" &&
    entitlements !== null &&
    "items" in entitlements &&
    Array.isArray(entitlements.items) &&
    entitlements.items.length > 0
  );
}

/** @param {unknown} profile */
export function normalizeMinecraftProfile(profile) {
  if (typeof profile !== "object" || profile === null) return null;
  const candidate = /** @type {{ id?: unknown, name?: unknown }} */ (profile);
  if (
    typeof candidate.id !== "string" ||
    !/^[a-f0-9]{32}$/i.test(candidate.id) ||
    typeof candidate.name !== "string" ||
    candidate.name.length === 0
  ) {
    return null;
  }

  const id = candidate.id.toLowerCase();
  const uuid = `${id.slice(0, 8)}-${id.slice(8, 12)}-${id.slice(12, 16)}-${id.slice(16, 20)}-${id.slice(20)}`;
  return { id, uuid, name: candidate.name };
}

/**
 * @param {{
 *   profile: { id: string, uuid: string, name: string },
 *   xuid: string,
 *   clientId: string,
 *   accessToken: string,
 *   expiresAt: number,
 *   createdAt: string,
 * }} input
 */
export function buildMinecraftHandoffPayload({
  profile,
  xuid,
  clientId,
  accessToken,
  expiresAt,
  createdAt,
}) {
  const accountId = `microsoft-${profile.uuid}`;
  return {
    account: {
      id: accountId,
      uuid: profile.uuid,
      username: profile.name,
      kind: "microsoft",
      uuid_prefix: profile.id,
      selected: false,
      created_at: createdAt,
    },
    secrets: {
      account_id: accountId,
      access_token: accessToken,
      session_server: null,
      xuid,
      client_id: clientId,
      expires_at: expiresAt,
    },
  };
}
