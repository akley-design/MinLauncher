import assert from "node:assert/strict";
import test from "node:test";

import { ACTIVITY_CURVES } from "../_shared/activity-levels.mjs";
import {
  createActivityProgressHandler,
  MAX_ACTIVITY_REQUEST_BYTES,
} from "./handler.mjs";

const USER_ID = "7b7c68b1-12d1-4f7e-a299-a89c63250b31";
const SESSION_ID = "45d615bb-d572-41cd-a77d-0b46d902a331";
const TOKEN = "header.payload.signature";

function fixture({ userId = USER_ID, rpcResult } = {}) {
  const calls = [];
  const logs = [];
  const handler = createActivityProgressHandler({
    async getUser(token) {
      assert.equal(token, TOKEN);
      return userId;
    },
    async rpc(name, args) {
      calls.push({ name, args });
      if (typeof rpcResult === "function") return await rpcResult(name, args);
      return rpcResult ?? { data: { session_id: SESSION_ID } };
    },
    log: (...event) => logs.push(event),
  });
  return { handler, calls, logs };
}

function post(body, { token = TOKEN, headers = {}, rawBody } = {}) {
  return new Request("https://example.test/functions/v1/activity-progress", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      ...(token ? { authorization: `Bearer ${token}` } : {}),
      ...headers,
    },
    body: rawBody ?? JSON.stringify(body),
  });
}

test("requires POST JSON and answers preflight without contacting auth or the database", async () => {
  const { handler, calls } = fixture();
  const getResponse = await handler(
    new Request("https://example.test", { method: "GET" }),
  );
  assert.equal(getResponse.status, 405);

  const textResponse = await handler(
    new Request("https://example.test", {
      method: "POST",
      headers: {
        authorization: `Bearer ${TOKEN}`,
        "content-type": "text/plain",
      },
      body: "{}",
    }),
  );
  assert.equal(textResponse.status, 415);

  const preflight = await handler(
    new Request("https://example.test", { method: "OPTIONS" }),
  );
  assert.equal(preflight.status, 200);
  assert.equal(calls.length, 0);
});

test("rejects oversized, malformed JSON, and invalid UTF-8 bodies", async () => {
  const { handler, calls } = fixture();
  const oversizedByHeader = await handler(
    post(
      {},
      { headers: { "content-length": String(MAX_ACTIVITY_REQUEST_BYTES + 1) } },
    ),
  );
  assert.equal(oversizedByHeader.status, 413);

  const oversizedStream = new Request("https://example.test", {
    method: "POST",
    headers: {
      authorization: `Bearer ${TOKEN}`,
      "content-type": "application/json",
    },
    body: new ReadableStream({
      start(controller) {
        controller.enqueue(new Uint8Array(MAX_ACTIVITY_REQUEST_BYTES + 1));
        controller.close();
      },
    }),
    duplex: "half",
  });
  assert.equal((await handler(oversizedStream)).status, 413);

  assert.equal((await handler(post(null, { rawBody: "{" }))).status, 400);
  const invalidUtf8 = new Request("https://example.test", {
    method: "POST",
    headers: {
      authorization: `Bearer ${TOKEN}`,
      "content-type": "application/json",
    },
    body: new Uint8Array([0xff]),
  });
  assert.equal((await handler(invalidUtf8)).status, 400);
  assert.equal(calls.length, 0);
});

test("rejects unauthenticated, malformed, and user-selected identities", async () => {
  const { handler, calls } = fixture({ userId: null });
  assert.equal(
    (await handler(post({ action: "progress" }, { token: null }))).status,
    401,
  );
  assert.equal((await handler(post({ action: "progress" }))).status, 401);
  assert.equal(
    (
      await handler(
        post({
          action: "progress",
          user_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        }),
      )
    ).status,
    400,
  );
  assert.equal(
    (await handler(post({ action: "start", source: "bot" }))).status,
    400,
  );
  assert.equal(
    (await handler(post({ action: "heartbeat", session_id: "bad", seq: 1 })))
      .status,
    400,
  );
  assert.equal(
    (
      await handler(
        post({ action: "heartbeat", session_id: SESSION_ID, seq: 1.5 }),
      )
    ).status,
    400,
  );
  assert.equal(calls.length, 0);
});

test("rejects non-string session IDs without coercion or RPC calls", async () => {
  const { handler, calls } = fixture();
  const malformedBodies = [
    { action: "heartbeat", session_id: [SESSION_ID], seq: 1 },
    { action: "heartbeat", session_id: { toString: "not-callable" }, seq: 1 },
    { action: "pause", session_id: [SESSION_ID] },
  ];

  for (const body of malformedBodies) {
    assert.equal((await handler(post(body, { token: null }))).status, 400);
  }
  assert.equal(calls.length, 0);
});

test("rejects malformed atlas loadouts before authentication or database access", async () => {
  const { handler, calls } = fixture();
  const invalid = [
    { action: "set_loadout", reward_ids: "atlas_pedestal" },
    { action: "set_loadout", reward_ids: ["atlas_pedestal", "atlas_pedestal"] },
    {
      action: "set_loadout",
      reward_ids: ["atlas_a", "atlas_b", "atlas_c", "atlas_d", "atlas_e"],
    },
    { action: "set_loadout", reward_ids: ["profile_frame_novice"] },
    {
      action: "set_loadout",
      reward_ids: ["atlas_pedestal"],
      user_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
    },
  ];

  for (const body of invalid) {
    assert.equal((await handler(post(body))).status, 400);
  }
  assert.equal(calls.length, 0);
});

test("atlas loadout rejects locked rewards and never writes when the account lacks the level", async () => {
  const { handler, calls } = fixture({
    rpcResult: async (name) => {
      assert.equal(name, "minlauncher_activity_get_progress");
      return {
        data: {
          total_milliseconds: String(ACTIVITY_CURVES[1][8].totalSeconds * 1000),
          curve_version: 1,
          equipped_atlas_ids: [],
        },
      };
    },
  });

  const response = await handler(
    post({ action: "set_loadout", reward_ids: ["atlas_pedestal"] }),
  );
  assert.equal(response.status, 403);
  assert.deepEqual(await response.json(), { error: "Atlas reward is locked" });
  assert.deepEqual(calls, [
    {
      name: "minlauncher_activity_get_progress",
      args: { p_user_id: USER_ID },
    },
  ]);
});

test("atlas loadout is saved only for the authenticated owner and returns server progress", async () => {
  const unlockedSeconds = ACTIVITY_CURVES[1][10].totalSeconds;
  const { handler, calls } = fixture({
    rpcResult: async (name, args) => {
      if (name === "minlauncher_activity_get_progress") {
        return {
          data: {
            total_milliseconds: String(unlockedSeconds * 1000),
            curve_version: 1,
            equipped_atlas_ids: [],
          },
        };
      }
      assert.equal(name, "minlauncher_activity_set_atlas_loadout");
      assert.deepEqual(args, {
        p_user_id: USER_ID,
        p_reward_ids: ["atlas_pedestal", "atlas_moss"],
      });
      return { data: { equipped_atlas_ids: args.p_reward_ids } };
    },
  });

  const response = await handler(
    post({
      action: "set_loadout",
      reward_ids: ["atlas_pedestal", "atlas_moss"],
    }),
  );
  assert.equal(response.status, 200);
  const payload = await response.json();
  assert.equal(payload.level, 11);
  assert.deepEqual(payload.equippedAtlasIds, ["atlas_pedestal", "atlas_moss"]);
  assert.deepEqual(
    calls.map(({ name }) => name),
    [
      "minlauncher_activity_get_progress",
      "minlauncher_activity_set_atlas_loadout",
    ],
  );
});

test("atlas loadout fails closed when the database returns a different selection", async () => {
  const { handler } = fixture({
    rpcResult: async (name) => {
      if (name === "minlauncher_activity_get_progress") {
        return {
          data: {
            total_milliseconds: String(
              ACTIVITY_CURVES[1][10].totalSeconds * 1000,
            ),
            curve_version: 1,
            equipped_atlas_ids: [],
          },
        };
      }
      return { data: { equipped_atlas_ids: ["atlas_pedestal"] } };
    },
  });

  const response = await handler(
    post({ action: "set_loadout", reward_ids: ["atlas_moss"] }),
  );
  assert.equal(response.status, 503);
  assert.deepEqual(await response.json(), {
    error: "Activity service is temporarily unavailable",
  });
});

test("the authenticated identity supplies the only owner ID for starting a session", async () => {
  const { handler, calls } = fixture();
  const response = await handler(post({ action: "start", source: "site" }));
  assert.equal(response.status, 200);
  assert.deepEqual(await response.json(), { sessionId: SESSION_ID });
  assert.deepEqual(calls, [
    {
      name: "minlauncher_activity_start",
      args: { p_user_id: USER_ID, p_source: "site" },
    },
  ]);
});

test("heartbeat requests forward only a validated sequence and session ID", async () => {
  const progress = {
    total_milliseconds: String(ACTIVITY_CURVES[1][0].totalSeconds * 1000),
    curve_version: 1,
    credited_milliseconds: 20_000,
    interval_status: "accepted",
    duplicate: false,
  };
  const { handler, calls } = fixture({ rpcResult: { data: progress } });
  const response = await handler(
    post({ action: "heartbeat", session_id: SESSION_ID, seq: 2 }),
  );
  assert.equal(response.status, 200);
  const payload = await response.json();
  assert.equal(payload.level, 1);
  assert.equal(payload.confirmedSeconds, ACTIVITY_CURVES[1][0].totalSeconds);
  assert.equal(payload.creditedMilliseconds, 20_000);
  assert.equal(payload.nextLevel, 2);
  assert.equal(payload.currentReward.rewardId, "profile_frame_novice");
  assert.deepEqual(calls, [
    {
      name: "minlauncher_activity_heartbeat",
      args: { p_user_id: USER_ID, p_session_id: SESSION_ID, p_seq: 2 },
    },
  ]);
});

test("progress is derived from persisted server milliseconds, and service failures stay generic", async () => {
  const totalMilliseconds = ACTIVITY_CURVES[1][9].totalSeconds * 1000 + 500;
  const { handler } = fixture({
    rpcResult: {
      data: {
        total_milliseconds: String(totalMilliseconds),
        curve_version: 1,
      },
    },
  });
  const progress = await handler(post({ action: "progress" }));
  assert.equal(progress.status, 200);
  const payload = await progress.json();
  assert.equal(payload.level, 10);
  assert.equal(payload.totalMilliseconds, totalMilliseconds);
  assert.equal(payload.secondsIntoLevel, 0);

  const failing = fixture({
    rpcResult: {
      error: { code: "XX000", message: "sensitive internal detail" },
    },
  });
  const errorResponse = await failing.handler(post({ action: "progress" }));
  assert.equal(errorResponse.status, 503);
  assert.doesNotMatch(
    await errorResponse.text(),
    /sensitive internal detail|XX000/,
  );
  assert.deepEqual(failing.logs, [["activity_rpc_failed", "XX000"]]);
});
