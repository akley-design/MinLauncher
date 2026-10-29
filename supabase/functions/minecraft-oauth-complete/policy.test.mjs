import assert from "node:assert/strict";
import test from "node:test";
import {
  buildMinecraftHandoffPayload,
  hasMinecraftEntitlement,
  minecraftProviderErrorCategory,
  normalizeMinecraftProfile,
} from "./policy.mjs";

test("Microsoft cancellation returns the cancellation state", () => {
  assert.equal(minecraftProviderErrorCategory("access_denied"), "cancelled");
  assert.equal(
    minecraftProviderErrorCategory("invalid_request"),
    "provider_error",
  );
});

test("Minecraft entitlement must include at least one owned item", () => {
  assert.equal(hasMinecraftEntitlement({ items: [{ name: "game" }] }), true);
  assert.equal(hasMinecraftEntitlement({ items: [] }), false);
  assert.equal(hasMinecraftEntitlement({}), false);
});

test("only a valid named Java profile becomes a game identity", () => {
  assert.deepEqual(
    normalizeMinecraftProfile({
      id: "AABBCCDDEEFF00112233445566778899",
      name: "Player",
    }),
    {
      id: "aabbccddeeff00112233445566778899",
      uuid: "aabbccdd-eeff-0011-2233-445566778899",
      name: "Player",
    },
  );
  assert.equal(normalizeMinecraftProfile({ id: "bad", name: "Player" }), null);
  assert.equal(
    normalizeMinecraftProfile({ id: "aabbccddeeff00112233445566778899" }),
    null,
  );
});

test("licensed profiles receive a stable account payload without refresh credentials", () => {
  const payload = buildMinecraftHandoffPayload({
    profile: {
      id: "aabbccddeeff00112233445566778899",
      uuid: "aabbccdd-eeff-0011-2233-445566778899",
      name: "Player",
    },
    xuid: "12345",
    clientId: "application-id",
    accessToken: "minecraft-access-token",
    expiresAt: 1_793_203_200,
    createdAt: "2026-10-28T00:00:00.000Z",
  });

  assert.equal(
    payload.account.id,
    "microsoft-aabbccdd-eeff-0011-2233-445566778899",
  );
  assert.equal(payload.account.kind, "microsoft");
  assert.equal(payload.secrets.access_token, "minecraft-access-token");
  assert.equal(payload.secrets.expires_at, 1_793_203_200);
  assert.equal("refresh_token" in payload.secrets, false);
  assert.equal("client_token" in payload.secrets, false);
});
