import {
  ACTIVITY_CURVES,
  getActivityProgress,
} from "../_shared/activity-levels.mjs";

export const MAX_ACTIVITY_REQUEST_BYTES = 4096;
const MAX_SEQUENCE = 2_147_483_647;
const MAX_ATLAS_EQUIPPED = 4;
const UUID_PATTERN =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const AUTH_TOKEN_PATTERN = /^[^\s]{1,8192}$/;

const corsHeaders = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers":
    "authorization, apikey, x-client-info, content-type",
  "Access-Control-Allow-Methods": "POST, OPTIONS",
  "Cache-Control": "no-store",
  Vary: "Origin",
};

function json(body, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      ...corsHeaders,
      "Content-Type": "application/json; charset=utf-8",
    },
  });
}

function isRecord(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

async function readBoundedBody(request) {
  const contentLength = request.headers.get("content-length");
  if (contentLength && /^\d+$/.test(contentLength)) {
    if (Number(contentLength) > MAX_ACTIVITY_REQUEST_BYTES)
      return { tooLarge: true };
  }

  if (!request.body) return { text: "" };
  const reader = request.body.getReader();
  const chunks = [];
  let size = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > MAX_ACTIVITY_REQUEST_BYTES) {
        await reader.cancel();
        return { tooLarge: true };
      }
      chunks.push(value);
    }
  } catch {
    return { invalid: true };
  }

  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }

  try {
    return { text: new TextDecoder("utf-8", { fatal: true }).decode(bytes) };
  } catch {
    return { invalid: true };
  }
}

function parseRequest(value) {
  if (!isRecord(value) || typeof value.action !== "string") return null;

  const keys = Object.keys(value);
  if (value.action === "start") {
    if (
      keys.length !== 2 ||
      !keys.includes("source") ||
      (value.source !== "site" && value.source !== "launcher")
    )
      return null;
    return { action: "start", source: value.source };
  }

  if (value.action === "progress") {
    return keys.length === 1 ? { action: "progress" } : null;
  }

  if (value.action === "set_loadout") {
    if (
      keys.length !== 2 ||
      !Array.isArray(value.reward_ids) ||
      value.reward_ids.length > MAX_ATLAS_EQUIPPED ||
      !value.reward_ids.every(
        (rewardId) =>
          typeof rewardId === "string" &&
          /^atlas_[a-z0-9_]{1,58}$/.test(rewardId),
      ) ||
      new Set(value.reward_ids).size !== value.reward_ids.length
    )
      return null;
    return { action: "set_loadout", rewardIds: [...value.reward_ids] };
  }

  if (value.action === "pause") {
    if (
      keys.length !== 2 ||
      typeof value.session_id !== "string" ||
      !UUID_PATTERN.test(value.session_id)
    )
      return null;
    return { action: "pause", sessionId: value.session_id };
  }

  if (value.action === "heartbeat") {
    if (
      keys.length !== 3 ||
      typeof value.session_id !== "string" ||
      !UUID_PATTERN.test(value.session_id) ||
      !Number.isSafeInteger(value.seq) ||
      value.seq < 1 ||
      value.seq > MAX_SEQUENCE
    )
      return null;
    return {
      action: "heartbeat",
      sessionId: value.session_id,
      sequence: value.seq,
    };
  }

  return null;
}

function progressPayload(data, creditedMilliseconds = 0) {
  if (!isRecord(data))
    throw new TypeError("Invalid activity progress response");
  const total = data.total_milliseconds;
  const totalBigInt =
    typeof total === "string" && /^\d+$/.test(total)
      ? BigInt(total)
      : typeof total === "number" && Number.isSafeInteger(total) && total >= 0
        ? BigInt(total)
        : null;
  if (totalBigInt === null || totalBigInt > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new TypeError("Invalid activity total");
  }
  const curveVersion = data.curve_version;
  if (!Number.isSafeInteger(curveVersion)) {
    throw new TypeError("Invalid activity curve version");
  }

  const totalMilliseconds = Number(totalBigInt);
  const totalSeconds = Math.floor(totalMilliseconds / 1000);
  const calculated = getActivityProgress(totalSeconds, curveVersion);
  const curve = ACTIVITY_CURVES[curveVersion];
  const equipped =
    data.equipped_atlas_ids === undefined ? [] : data.equipped_atlas_ids;
  if (
    !Array.isArray(equipped) ||
    equipped.length > MAX_ATLAS_EQUIPPED ||
    !equipped.every(
      (rewardId) =>
        typeof rewardId === "string" &&
        /^atlas_[a-z0-9_]{1,58}$/.test(rewardId) &&
        calculated.unlockedRewardIds.includes(rewardId),
    ) ||
    new Set(equipped).size !== equipped.length
  ) {
    throw new TypeError("Invalid atlas loadout");
  }
  return {
    ...calculated,
    totalMilliseconds,
    confirmedSeconds: totalSeconds,
    creditedMilliseconds,
    equippedAtlasIds: [...equipped],
    currentReward: calculated.level > 0 ? curve[calculated.level - 1] : null,
    nextReward: calculated.nextLevel ? curve[calculated.nextLevel - 1] : null,
  };
}

function rpcErrorCode(error) {
  return isRecord(error) && typeof error.code === "string"
    ? error.code
    : "unknown";
}

/**
 * Build a testable request handler. Authentication must return the verified
 * Supabase user ID or null; RPC calls must use a server-side service client.
 * @param {{getUser: (token: string) => Promise<string | null>, rpc: (name: string, args: Record<string, unknown>) => Promise<{data?: unknown, error?: unknown}>, log?: (event: string, code: string) => void}} dependencies
 */
export function createActivityProgressHandler({
  getUser,
  rpc,
  log = () => {},
}) {
  return async function handleActivityProgress(request) {
    if (request.method === "OPTIONS")
      return new Response("ok", { headers: corsHeaders });
    if (request.method !== "POST")
      return json({ error: "Method not allowed" }, 405);

    const contentType = request.headers.get("content-type") ?? "";
    if (!/^application\/json(?:\s*;|$)/i.test(contentType)) {
      return json({ error: "Content-Type must be application/json" }, 415);
    }

    const bodyResult = await readBoundedBody(request);
    if (bodyResult.tooLarge)
      return json({ error: "Request is too large" }, 413);
    if (bodyResult.invalid) return json({ error: "Invalid request" }, 400);

    let parsed;
    try {
      parsed = JSON.parse(bodyResult.text);
    } catch {
      return json({ error: "Invalid request" }, 400);
    }
    const operation = parseRequest(parsed);
    if (!operation) return json({ error: "Invalid request" }, 400);

    const authorization = request.headers.get("authorization") ?? "";
    const match = /^Bearer (.+)$/i.exec(authorization);
    if (!match || !AUTH_TOKEN_PATTERN.test(match[1])) {
      return json({ error: "Sign in required" }, 401);
    }

    let userId;
    try {
      userId = await getUser(match[1]);
    } catch {
      return json(
        { error: "Activity service is temporarily unavailable" },
        503,
      );
    }
    if (typeof userId !== "string" || !UUID_PATTERN.test(userId)) {
      return json({ error: "Sign in required" }, 401);
    }

    if (operation.action === "set_loadout") {
      let progressResult;
      try {
        progressResult = await rpc("minlauncher_activity_get_progress", {
          p_user_id: userId,
        });
      } catch (error) {
        log("activity_rpc_failed", rpcErrorCode(error));
        return json(
          { error: "Activity service is temporarily unavailable" },
          503,
        );
      }
      if (progressResult?.error) {
        const code = rpcErrorCode(progressResult.error);
        log("activity_rpc_failed", code);
        return json(
          { error: "Activity service is temporarily unavailable" },
          503,
        );
      }

      let currentProgress;
      try {
        currentProgress = progressPayload(progressResult?.data);
      } catch {
        log("activity_invalid_server_response", "loadout_progress");
        return json(
          { error: "Activity service is temporarily unavailable" },
          503,
        );
      }
      if (
        operation.rewardIds.some(
          (rewardId) => !currentProgress.unlockedRewardIds.includes(rewardId),
        )
      ) {
        return json({ error: "Atlas reward is locked" }, 403);
      }

      let loadoutResult;
      try {
        loadoutResult = await rpc("minlauncher_activity_set_atlas_loadout", {
          p_user_id: userId,
          p_reward_ids: operation.rewardIds,
        });
      } catch (error) {
        log("activity_rpc_failed", rpcErrorCode(error));
        return json(
          { error: "Activity service is temporarily unavailable" },
          503,
        );
      }
      if (loadoutResult?.error) {
        const code = rpcErrorCode(loadoutResult.error);
        log("activity_rpc_failed", code);
        return json(
          { error: "Activity service is temporarily unavailable" },
          503,
        );
      }

      try {
        const data = loadoutResult?.data;
        if (!isRecord(data) || !Array.isArray(data.equipped_atlas_ids))
          throw new TypeError("Invalid atlas loadout response");
        if (
          data.equipped_atlas_ids.length !== operation.rewardIds.length ||
          data.equipped_atlas_ids.some(
            (rewardId, index) => rewardId !== operation.rewardIds[index],
          )
        ) {
          throw new TypeError("Atlas loadout response does not match request");
        }
        return json(
          progressPayload({
            ...progressResult.data,
            equipped_atlas_ids: data.equipped_atlas_ids,
          }),
        );
      } catch {
        log("activity_invalid_server_response", "loadout_save");
        return json(
          { error: "Activity service is temporarily unavailable" },
          503,
        );
      }
    }

    let functionName;
    let args;
    switch (operation.action) {
      case "start":
        functionName = "minlauncher_activity_start";
        args = { p_user_id: userId, p_source: operation.source };
        break;
      case "heartbeat":
        functionName = "minlauncher_activity_heartbeat";
        args = {
          p_user_id: userId,
          p_session_id: operation.sessionId,
          p_seq: operation.sequence,
        };
        break;
      case "pause":
        functionName = "minlauncher_activity_pause";
        args = { p_user_id: userId, p_session_id: operation.sessionId };
        break;
      case "progress":
        functionName = "minlauncher_activity_get_progress";
        args = { p_user_id: userId };
        break;
    }

    let result;
    try {
      result = await rpc(functionName, args);
    } catch (error) {
      log("activity_rpc_failed", rpcErrorCode(error));
      return json(
        { error: "Activity service is temporarily unavailable" },
        503,
      );
    }
    if (result?.error) {
      const code = rpcErrorCode(result.error);
      log("activity_rpc_failed", code);
      return json(
        { error: "Activity service is temporarily unavailable" },
        503,
      );
    }

    const data = result?.data;
    if (operation.action === "start") {
      if (!isRecord(data) || !UUID_PATTERN.test(data.session_id ?? "")) {
        log("activity_invalid_server_response", "start");
        return json(
          { error: "Activity service is temporarily unavailable" },
          503,
        );
      }
      return json({ sessionId: data.session_id });
    }
    if (operation.action === "pause") {
      return json({ paused: isRecord(data) && data.paused === true });
    }
    if (isRecord(data) && data.error_code === "session_unavailable") {
      return json({ error: "Activity session is unavailable" }, 404);
    }
    if (isRecord(data) && data.error_code === "stale_sequence") {
      return json({ error: "Heartbeat sequence is stale" }, 409);
    }
    if (isRecord(data) && data.error_code === "start_rate_limited") {
      return json({ error: "Too many activity sessions" }, 429);
    }
    if (operation.action === "heartbeat") {
      const credited = data?.credited_milliseconds;
      if (
        !Number.isSafeInteger(credited) ||
        credited < 0 ||
        credited > 45_000
      ) {
        log("activity_invalid_server_response", "heartbeat");
        return json(
          { error: "Activity service is temporarily unavailable" },
          503,
        );
      }
      try {
        return json({
          ...progressPayload(data, credited),
          duplicate: data.duplicate === true,
          intervalStatus:
            typeof data.interval_status === "string"
              ? data.interval_status
              : "accepted",
        });
      } catch {
        log("activity_invalid_server_response", "heartbeat_progress");
        return json(
          { error: "Activity service is temporarily unavailable" },
          503,
        );
      }
    }

    try {
      return json(progressPayload(data));
    } catch {
      log("activity_invalid_server_response", "progress");
      return json(
        { error: "Activity service is temporarily unavailable" },
        503,
      );
    }
  };
}
