import { createClient } from "npm:@supabase/supabase-js@2";
import { corsHeaders, json } from "../_shared/http.ts";
import { normalizeNickname } from "./nickname.mjs";
import { isAllowedLoginOrigin, signInByNickname } from "./signin.mjs";

const validName = (value: unknown): value is string =>
  normalizeNickname(value) !== null;

async function sha256(value: string): Promise<string> {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(value),
  );
  return Array.from(new Uint8Array(digest), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
}

Deno.serve(async (request: Request) => {
  if (request.method === "OPTIONS")
    return new Response("ok", { headers: corsHeaders });
  if (request.method !== "POST")
    return json({ error: "Method not allowed" }, 405);
  const url = Deno.env.get("SUPABASE_URL");
  const anonKey = Deno.env.get("SUPABASE_ANON_KEY");
  const serviceKey = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY");
  if (!url || !anonKey || !serviceKey)
    return json({ error: "Server configuration is missing" }, 500);
  const raw = await request.text();
  if (raw.length > 4096) return json({ error: "Request is too large" }, 413);
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return json({ error: "Invalid request" }, 400);
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed))
    return json({ error: "Invalid request" }, 400);
  const body = parsed as Record<string, unknown>;
  const operation = body.operation;
  const name = body.nickname;
  if (
    (operation === "check" ||
      operation === "claim_offline" ||
      operation === "claim_profile") &&
    !validName(name)
  ) {
    return json(
      {
        error: "Ник должен содержать 3–16 латинских букв, цифр или символов _",
      },
      400,
    );
  }
  const normalized = normalizeNickname(name) ?? "";
  const admin = createClient(url, serviceKey, {
    auth: { persistSession: false, autoRefreshToken: false },
  });

  if (operation === "sign_in") {
    const origin = request.headers.get("Origin");
    if (origin && !isAllowedLoginOrigin(origin)) {
      return new Response(JSON.stringify({ error: "Request not allowed" }), {
        status: 403,
        headers: {
          "Content-Type": "application/json; charset=utf-8",
          "Cache-Control": "no-store",
          Vary: "Origin",
        },
      });
    }
    const authJson = (response: unknown, status: number) => {
      const headers = new Headers(corsHeaders);
      headers.set("Content-Type", "application/json; charset=utf-8");
      headers.set("Cache-Control", "no-store");
      headers.set("Vary", "Origin");
      if (origin) headers.set("Access-Control-Allow-Origin", origin);
      return new Response(JSON.stringify(response), { status, headers });
    };
    try {
      const result = await signInByNickname({
        admin,
        supabaseUrl: url,
        anonKey,
        nickname: body.nickname,
        password: body.password,
      });
      return authJson(result.body, result.status);
    } catch {
      return authJson(
        { error: "Вход временно недоступен. Попробуйте позже." },
        503,
      );
    }
  }

  if (operation === "check") {
    const { data, error } = await admin.rpc("minlauncher_check_nickname", {
      p_nickname: normalized,
    });
    if (error) return json({ error: "Не удалось проверить ник" }, 500);
    return json({ available: data === true });
  }
  if (operation === "claim_offline") {
    const playerUuid = body.player_uuid;
    if (typeof playerUuid !== "string" || !/^[0-9a-f-]{36}$/i.test(playerUuid))
      return json({ error: "Invalid player UUID" }, 400);
    const claimToken =
      typeof body.claim_token === "string" &&
      /^[a-f0-9]{64}$/.test(body.claim_token)
        ? body.claim_token
        : Array.from(crypto.getRandomValues(new Uint8Array(32)), (byte) =>
            byte.toString(16).padStart(2, "0"),
          ).join("");
    const { data, error } = await admin.rpc("minlauncher_claim_offline", {
      p_nickname: name,
      p_nickname_normalized: normalized,
      p_claim_token_hash: await sha256(claimToken),
      p_player_uuid: playerUuid,
    });
    if (error) return json({ error: "Не удалось зарезервировать ник" }, 500);
    if (!Array.isArray(data) || data.length === 0)
      return json({ error: "Этот ник уже занят" }, 409);
    return json({
      nickname: data[0].nickname,
      player_uuid: data[0].player_uuid,
      claim_token: claimToken,
    });
  }
  if (operation === "release_offline") {
    const token = body.claim_token;
    if (typeof token !== "string" || !/^[a-f0-9]{64}$/.test(token))
      return json({ error: "Invalid claim token" }, 400);
    const { error } = await admin.rpc("minlauncher_release_offline", {
      p_claim_token_hash: await sha256(token),
    });
    return error
      ? json({ error: "Не удалось освободить ник" }, 500)
      : json({ released: true });
  }

  const authorization = request.headers.get("Authorization");
  if (!authorization?.startsWith("Bearer "))
    return json({ error: "Sign in required" }, 401);
  const userClient = createClient(url, anonKey, {
    global: { headers: { Authorization: authorization } },
    auth: { persistSession: false, autoRefreshToken: false },
  });
  const {
    data: { user },
    error: userError,
  } = await userClient.auth.getUser();
  if (userError || !user) return json({ error: "Sign in required" }, 401);
  if (operation === "get_profile") {
    const { data, error } = await admin
      .from("minlauncher_nicknames")
      .select("nickname,player_uuid")
      .eq("owner_kind", "registered")
      .eq("user_id", user.id)
      .maybeSingle();
    if (error) return json({ error: "Не удалось прочитать профиль" }, 500);
    return json({ profile: data });
  }
  if (operation === "claim_profile") {
    const token = body.claim_token;
    if (
      token !== undefined &&
      (typeof token !== "string" || !/^[a-f0-9]{64}$/.test(token))
    )
      return json({ error: "Invalid claim token" }, 400);
    const { data, error } = await admin.rpc("minlauncher_claim_registered", {
      p_user_id: user.id,
      p_nickname: name,
      p_nickname_normalized: normalized,
      p_claim_token_hash: token ? await sha256(token as string) : null,
    });
    if (error) return json({ error: "Не удалось сохранить ник" }, 500);
    if (!Array.isArray(data) || data.length === 0)
      return json({ error: "Этот ник уже занят" }, 409);
    return json({
      profile: { nickname: data[0].nickname, player_uuid: data[0].player_uuid },
    });
  }
  return json({ error: "Unknown operation" }, 400);
});
