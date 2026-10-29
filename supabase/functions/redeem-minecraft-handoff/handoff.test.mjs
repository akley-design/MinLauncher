import assert from "node:assert/strict";
import test from "node:test";
import { consumeMinecraftHandoff } from "./handoff.mjs";

function createAdmin({ expired = false, fail = false } = {}) {
  const state = { consumed: false };
  return {
    from() {
      const query = {
        update() {
          return query;
        },
        eq() {
          return query;
        },
        is() {
          return query;
        },
        gt() {
          return query;
        },
        select() {
          return query;
        },
        async maybeSingle() {
          if (fail) return { data: null, error: new Error("database failure") };
          if (expired || state.consumed) return { data: null, error: null };
          state.consumed = true;
          return {
            data: { encrypted_payload: "encrypted-payload" },
            error: null,
          };
        },
      };
      return query;
    },
  };
}

test("a valid Minecraft handoff returns the encrypted account payload", async () => {
  const result = await consumeMinecraftHandoff(
    createAdmin(),
    "hash",
    new Date("2026-10-28T00:00:00.000Z"),
  );
  assert.deepEqual(result, {
    kind: "ok",
    encryptedPayload: "encrypted-payload",
  });
});

test("expired and reused Minecraft handoffs are rejected", async () => {
  assert.deepEqual(
    await consumeMinecraftHandoff(
      createAdmin({ expired: true }),
      "hash",
      new Date("2026-10-28T00:00:00.000Z"),
    ),
    { kind: "expired_or_used" },
  );

  const admin = createAdmin();
  const now = new Date("2026-10-28T00:00:00.000Z");
  assert.equal((await consumeMinecraftHandoff(admin, "hash", now)).kind, "ok");
  assert.deepEqual(await consumeMinecraftHandoff(admin, "hash", now), {
    kind: "expired_or_used",
  });
});

test("database errors fail closed", async () => {
  assert.deepEqual(
    await consumeMinecraftHandoff(
      createAdmin({ fail: true }),
      "hash",
      new Date("2026-10-28T00:00:00.000Z"),
    ),
    { kind: "error" },
  );
});
