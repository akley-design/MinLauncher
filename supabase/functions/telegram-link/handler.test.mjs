import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";

let createTelegramLinkHandler;
try {
  ({ createTelegramLinkHandler } = await import("./handler.mjs"));
} catch (error) {
  if (error?.code !== "ERR_MODULE_NOT_FOUND") throw error;
}

const USER_ID = "1b5f5a73-36cf-4db5-9b93-46c5b6142bb8";
const TOKEN = "A".repeat(43);
const TOKEN_HASH = createHash("sha256").update(TOKEN).digest("hex");
const BOT_SECRET = "test-only-random-bot-secret";
const EXPIRES_AT = "2026-11-11T12:10:00.000Z";

function request(body, headers = {}, method = "POST") {
  return new Request("https://example.test/functions/v1/telegram-link", {
    method,
    headers: { "Content-Type": "application/json", ...headers },
    ...(method === "POST" ? { body: JSON.stringify(body) } : {}),
  });
}

function makeHandler(overrides = {}) {
  assert.equal(
    typeof createTelegramLinkHandler,
    "function",
    "telegram-link handler is not implemented",
  );
  const calls = [];
  const store = {
    findLinkByUserId: async (userId) => { calls.push(["findLinkByUserId", userId]); return null; },
    createChallenge: async (challenge) => { calls.push(["createChallenge", challenge]); },
    redeemChallenge: async (input) => { calls.push(["redeemChallenge", input]); return { status: "linked" }; },
    findUserIdByTelegramId: async (id) => { calls.push(["findUserIdByTelegramId", id]); return null; },
    getActivity: async (userId) => { calls.push(["getActivity", userId]); return { total_milliseconds: "3600000", curve_version: 1 }; },
    unlinkByUserId: async (userId) => { calls.push(["unlinkByUserId", userId]); },
    unlinkByTelegramId: async (id) => { calls.push(["unlinkByTelegramId", id]); },
    ...overrides.store,
  };
  const handler = createTelegramLinkHandler({
    botSecret: BOT_SECRET,
    getUser: async (token) => token === "valid-user-token" ? USER_ID : null,
    store,
    now: () => new Date("2026-11-11T12:00:00.000Z"),
    generateToken: () => TOKEN,
    ...overrides.dependencies,
  });
  return { handler, calls, store };
}

const USER_AUTH = { Authorization: "Bearer valid-user-token" };
const BOT_AUTH = { "X-MinLauncher-Bot-Secret": BOT_SECRET };

test("create stores only the token hash and expires the code after ten minutes", async () => {
  let persisted;
  const { handler } = makeHandler({
    store: { createChallenge: async (challenge) => { persisted = challenge; } },
  });
  const response = await handler(request({ action: "create" }, USER_AUTH));

  assert.equal(response.status, 200);
  assert.deepEqual(await response.json(), { payload: TOKEN, expiresAt: EXPIRES_AT });
  assert.deepEqual(persisted, {
    userId: USER_ID,
    tokenHash: TOKEN_HASH,
    expiresAt: EXPIRES_AT,
  });
  assert.equal(Object.hasOwn(persisted, "token"), false);
});

test("create rejects a caller without a verified user session", async () => {
  const { handler, calls } = makeHandler();
  const response = await handler(request({ action: "create" }, {
    Authorization: "Bearer invalid-token",
  }));

  assert.equal(response.status, 401);
  assert.deepEqual(await response.json(), { error: "unauthorized" });
  assert.deepEqual(calls, []);
});

test("create rejects a caller-supplied account ID", async () => {
  const { handler, calls } = makeHandler();
  const response = await handler(request({ action: "create", userId: "attacker" }, USER_AUTH));

  assert.equal(response.status, 400);
  assert.deepEqual(await response.json(), { error: "invalid_request" });
  assert.deepEqual(calls, []);
});

test("status returns only whether the authenticated account is linked", async () => {
  const { handler, calls } = makeHandler({
    store: { findLinkByUserId: async (userId) => { calls.push(["lookup", userId]); return { telegram_user_id: "123456789" }; } },
  });
  const response = await handler(request({ action: "status" }, USER_AUTH));

  assert.equal(response.status, 200);
  assert.deepEqual(await response.json(), { linked: true });
  assert.deepEqual(calls, [["lookup", USER_ID]]);
});

test("redeem rejects an incorrect bot secret before touching storage", async () => {
  const { handler, calls } = makeHandler();
  const response = await handler(request({ action: "redeem", payload: TOKEN, telegramUserId: "123456789" }, {
    "X-MinLauncher-Bot-Secret": "wrong-secret",
  }));

  assert.equal(response.status, 401);
  assert.deepEqual(await response.json(), { error: "unauthorized" });
  assert.deepEqual(calls, []);
});

test("redeem passes only a token hash and Telegram numeric ID to the atomic store", async () => {
  let redemption;
  const { handler } = makeHandler({
    store: { redeemChallenge: async (input) => { redemption = input; return { status: "linked" }; } },
  });
  const response = await handler(request({ action: "redeem", payload: TOKEN, telegramUserId: "123456789" }, BOT_AUTH));

  assert.equal(response.status, 200);
  assert.deepEqual(await response.json(), { linked: true });
  assert.deepEqual(redemption, { tokenHash: TOKEN_HASH, telegramUserId: "123456789" });
});

test("redeem reports a linked-account conflict without returning an account ID", async () => {
  const { handler } = makeHandler({
    store: { redeemChallenge: async () => ({ status: "already_linked" }) },
  });
  const response = await handler(request({ action: "redeem", payload: TOKEN, telegramUserId: "123456789" }, BOT_AUTH));

  assert.equal(response.status, 409);
  assert.deepEqual(await response.json(), { error: "already_linked" });
});

test("redeem reports an expired one-time code distinctly", async () => {
  const { handler } = makeHandler({
    store: { redeemChallenge: async () => ({ status: "expired" }) },
  });
  const response = await handler(request({ action: "redeem", payload: TOKEN, telegramUserId: "123456789" }, BOT_AUTH));

  assert.equal(response.status, 410);
  assert.deepEqual(await response.json(), { error: "link_expired" });
});

test("redeem reports a reused one-time code distinctly", async () => {
  const { handler } = makeHandler({
    store: { redeemChallenge: async () => ({ status: "used" }) },
  });
  const response = await handler(request({ action: "redeem", payload: TOKEN, telegramUserId: "123456789" }, BOT_AUTH));

  assert.equal(response.status, 409);
  assert.deepEqual(await response.json(), { error: "link_used" });
});

test("redeem rejects malformed token and Telegram ID before calling storage", async () => {
  const { handler, calls } = makeHandler();
  const response = await handler(request({ action: "redeem", payload: "bad token", telegramUserId: "-1" }, BOT_AUTH));

  assert.equal(response.status, 400);
  assert.deepEqual(await response.json(), { error: "invalid_request" });
  assert.deepEqual(calls, []);
});

test("progress returns a safe summary calculated from confirmed server activity", async () => {
  const { handler, calls } = makeHandler({
    store: {
      findUserIdByTelegramId: async (id) => { calls.push(["link", id]); return USER_ID; },
    },
  });
  const response = await handler(request({ action: "progress", telegramUserId: "123456789" }, BOT_AUTH));

  assert.equal(response.status, 200);
  assert.deepEqual(await response.json(), {
    linked: true,
    level: 2,
    confirmedSeconds: 3600,
    nextLevel: 3,
    secondsRemaining: 2100,
    currentReward: { rewardId: "accent_moss", rewardLabel: "Цветовой акцент «Мох»" },
    nextReward: { rewardId: "badge_traveler", rewardLabel: "Значок путешественника" },
  });
  assert.deepEqual(calls, [["link", "123456789"], ["getActivity", USER_ID]]);
});

test("progress for an unlinked Telegram account does not query activity", async () => {
  const { handler, calls } = makeHandler();
  const response = await handler(request({ action: "progress", telegramUserId: "123456789" }, BOT_AUTH));

  assert.equal(response.status, 200);
  assert.deepEqual(await response.json(), { linked: false });
  assert.deepEqual(calls, [["findUserIdByTelegramId", "123456789"]]);
});

test("site unlink removes only the currently authenticated account's link", async () => {
  const { handler, calls } = makeHandler();
  const response = await handler(request({ action: "unlink" }, USER_AUTH));

  assert.equal(response.status, 200);
  assert.deepEqual(await response.json(), { linked: false });
  assert.deepEqual(calls, [["unlinkByUserId", USER_ID]]);
});

test("bot unlink requires its secret and uses only the supplied Telegram ID", async () => {
  const { handler, calls } = makeHandler();
  const response = await handler(request({ action: "unlink", telegramUserId: "123456789" }, BOT_AUTH));

  assert.equal(response.status, 200);
  assert.deepEqual(await response.json(), { linked: false });
  assert.deepEqual(calls, [["unlinkByTelegramId", "123456789"]]);
});

test("database failures return a generic error and do not expose details", async () => {
  const { handler } = makeHandler({
    store: { findLinkByUserId: async () => { throw new Error("private database detail"); } },
  });
  const response = await handler(request({ action: "status" }, USER_AUTH));

  assert.equal(response.status, 503);
  const body = await response.text();
  assert.deepEqual(JSON.parse(body), { error: "service_unavailable" });
  assert.equal(body.includes("private database detail"), false);
});

test("preflight requests return CORS headers for the browser and bot secret", async () => {
  const { handler } = makeHandler();
  const response = await handler(request({}, {}, "OPTIONS"));

  assert.equal(response.status, 200);
  assert.match(response.headers.get("access-control-allow-headers"), /x-minlauncher-bot-secret/i);
  assert.match(response.headers.get("access-control-allow-methods"), /POST/);
});