import {
  ACTIVITY_CURVES,
  getActivityProgress,
} from "../_shared/activity-levels.mjs";

const MAX_REQUEST_BYTES = 4096;
const TOKEN_PATTERN = /^[A-Za-z0-9_-]{43}$/;
const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const TELEGRAM_ID_PATTERN = /^[1-9][0-9]{0,18}$/;
const MAX_BIGINT = 9_223_372_036_854_775_807n;
const BOT_HEADER = "x-minlauncher-bot-secret";

const corsHeaders = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers": "authorization, apikey, x-client-info, content-type, x-minlauncher-bot-secret",
  "Access-Control-Allow-Methods": "POST, OPTIONS",
  "Cache-Control": "no-store",
  Vary: "Origin",
};

function json(body, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { ...corsHeaders, "Content-Type": "application/json; charset=utf-8" },
  });
}

function isRecord(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

async function readBoundedBody(request) {
  const contentLength = request.headers.get("content-length");
  if (contentLength && /^\d+$/.test(contentLength) && Number(contentLength) > MAX_REQUEST_BYTES) {
    return { tooLarge: true };
  }
  if (!request.body) return { invalid: true };

  const reader = request.body.getReader();
  const chunks = [];
  let size = 0;
  while (true) {
    let result;
    try {
      result = await reader.read();
    } catch {
      return { invalid: true };
    }
    if (result.done) break;
    size += result.value.byteLength;
    if (size > MAX_REQUEST_BYTES) return { tooLarge: true };
    chunks.push(result.value);
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

function parseTelegramId(value) {
  if (typeof value !== "string" || !TELEGRAM_ID_PATTERN.test(value)) return null;
  try {
    const parsed = BigInt(value);
    return parsed > 0n && parsed <= MAX_BIGINT && parsed.toString() === value ? value : null;
  } catch {
    return null;
  }
}

function parseRequest(value) {
  if (!isRecord(value) || typeof value.action !== "string") return null;
  const keys = Object.keys(value).sort();
  const exactKeys = (expected) =>
    keys.length === expected.length && expected.every((key, index) => key === keys[index]);

  if (value.action === "status" || value.action === "create") {
    return exactKeys(["action"]) ? { action: value.action } : null;
  }
  if (value.action === "redeem") {
    if (!exactKeys(["action", "payload", "telegramUserId"])) return null;
    if (typeof value.payload !== "string" || !TOKEN_PATTERN.test(value.payload)) return null;
    const telegramUserId = parseTelegramId(value.telegramUserId);
    return telegramUserId ? { action: "redeem", payload: value.payload, telegramUserId } : null;
  }
  if (value.action === "progress") {
    if (!exactKeys(["action", "telegramUserId"])) return null;
    const telegramUserId = parseTelegramId(value.telegramUserId);
    return telegramUserId ? { action: "progress", telegramUserId } : null;
  }
  if (value.action === "unlink") {
    if (exactKeys(["action"])) return { action: "unlink", actor: "user" };
    if (!exactKeys(["action", "telegramUserId"])) return null;
    const telegramUserId = parseTelegramId(value.telegramUserId);
    return telegramUserId ? { action: "unlink", actor: "bot", telegramUserId } : null;
  }
  return null;
}

function base64Url(bytes) {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

function generateSecureToken() {
  return base64Url(crypto.getRandomValues(new Uint8Array(32)));
}

async function hashToken(token) {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(token));
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function sameSecret(candidate, expected) {
  if (!candidate || !expected) return false;
  const left = new TextEncoder().encode(candidate);
  const right = new TextEncoder().encode(expected);
  let difference = left.length ^ right.length;
  const length = Math.max(left.length, right.length);
  for (let index = 0; index < length; index += 1) {
    difference |= (left[index] ?? 0) ^ (right[index] ?? 0);
  }
  return difference === 0;
}

function rewardSummary(reward) {
  return reward ? { rewardId: reward.rewardId, rewardLabel: reward.rewardLabel } : null;
}

function progressSummary(activity) {
  if (!isRecord(activity)) throw new TypeError("Invalid activity record");
  const milliseconds = activity.total_milliseconds;
  if (typeof milliseconds !== "string" || !/^\d+$/.test(milliseconds)) {
    throw new TypeError("Invalid confirmed activity time");
  }
  const totalSecondsBigInt = BigInt(milliseconds) / 1000n;
  if (totalSecondsBigInt > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new TypeError("Confirmed activity time exceeds safe range");
  }
  const curveVersion = activity.curve_version;
  if (!Number.isSafeInteger(curveVersion)) throw new TypeError("Invalid activity curve version");
  const totalSeconds = Number(totalSecondsBigInt);
  const calculated = getActivityProgress(totalSeconds, curveVersion);
  const curve = ACTIVITY_CURVES[curveVersion];
  return {
    linked: true,
    level: calculated.level,
    confirmedSeconds: totalSeconds,
    nextLevel: calculated.nextLevel,
    secondsRemaining: calculated.secondsRemaining,
    currentReward: rewardSummary(calculated.level > 0 ? curve[calculated.level - 1] : null),
    nextReward: rewardSummary(calculated.nextLevel ? curve[calculated.nextLevel - 1] : null),
  };
}

function parseBearer(request) {
  const match = /^Bearer\s+([^\s]+)$/i.exec(request.headers.get("authorization") ?? "");
  return match?.[1] ?? null;
}

/**
 * Build the account-link handler. Database access and JWT verification are supplied by the Edge entry point.
 * @param {{botSecret: string, getUser: (token: string) => Promise<string | null>, store: Record<string, Function>, now?: () => Date, generateToken?: () => string, log?: (event: string, code: string) => void}} dependencies
 */
export function createTelegramLinkHandler({
  botSecret,
  getUser,
  store,
  now = () => new Date(),
  generateToken = generateSecureToken,
  log = () => {},
}) {
  return async function handleTelegramLink(request) {
    if (request.method === "OPTIONS") return json({ ok: true });
    if (request.method !== "POST") return json({ error: "method_not_allowed" }, 405);
    if (!/^application\/json(?:\s*;|$)/i.test(request.headers.get("content-type") ?? "")) {
      return json({ error: "invalid_request" }, 415);
    }

    const body = await readBoundedBody(request);
    if (body.tooLarge) return json({ error: "request_too_large" }, 413);
    if (body.invalid) return json({ error: "invalid_request" }, 400);
    let decoded;
    try {
      decoded = JSON.parse(body.text);
    } catch {
      return json({ error: "invalid_request" }, 400);
    }
    const operation = parseRequest(decoded);
    if (!operation) return json({ error: "invalid_request" }, 400);

    const botOperation = operation.action === "redeem" || operation.action === "progress" ||
      (operation.action === "unlink" && operation.actor === "bot");
    let userId = null;
    if (botOperation) {
      if (!sameSecret(request.headers.get(BOT_HEADER) ?? "", botSecret)) {
        return json({ error: "unauthorized" }, 401);
      }
    } else {
      const token = parseBearer(request);
      if (!token) return json({ error: "unauthorized" }, 401);
      try {
        userId = await getUser(token);
      } catch {
        log("telegram_link_auth_failed", "upstream_unavailable");
        return json({ error: "service_unavailable" }, 503);
      }
      if (typeof userId !== "string" || !UUID_PATTERN.test(userId)) {
        return json({ error: "unauthorized" }, 401);
      }
    }

    try {
      if (operation.action === "status") {
        const link = await store.findLinkByUserId(userId);
        return json({ linked: Boolean(link) });
      }

      if (operation.action === "create") {
        if (await store.findLinkByUserId(userId)) {
          return json({ error: "already_linked" }, 409);
        }
        const payload = generateToken();
        if (typeof payload !== "string" || !TOKEN_PATTERN.test(payload)) {
          throw new TypeError("Token generator returned invalid payload");
        }
        const expiresAt = new Date(new Date(now()).getTime() + 10 * 60 * 1000).toISOString();
        await store.createChallenge({ userId, tokenHash: await hashToken(payload), expiresAt });
        return json({ payload, expiresAt });
      }

      if (operation.action === "redeem") {
        const result = await store.redeemChallenge({
          tokenHash: await hashToken(operation.payload),
          telegramUserId: operation.telegramUserId,
        });
        const status = result?.status;
        if (status === "linked") return json({ linked: true });
        if (status === "expired") return json({ error: "link_expired" }, 410);
        if (status === "used") return json({ error: "link_used" }, 409);
        if (status === "already_linked") return json({ error: "already_linked" }, 409);
        if (status === "invalid") return json({ error: "invalid_link" }, 400);
        throw new TypeError("Unknown challenge redemption result");
      }

      if (operation.action === "progress") {
        const linkedUserId = await store.findUserIdByTelegramId(operation.telegramUserId);
        if (!linkedUserId) return json({ linked: false });
        if (typeof linkedUserId !== "string" || !UUID_PATTERN.test(linkedUserId)) {
          throw new TypeError("Invalid linked account identifier");
        }
        return json(progressSummary(await store.getActivity(linkedUserId)));
      }

      if (operation.actor === "bot") {
        await store.unlinkByTelegramId(operation.telegramUserId);
      } else {
        await store.unlinkByUserId(userId);
      }
      return json({ linked: false });
    } catch {
      log("telegram_link_request_failed", operation.action);
      return json({ error: "service_unavailable" }, 503);
    }
  };
}