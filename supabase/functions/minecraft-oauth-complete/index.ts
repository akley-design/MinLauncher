import { createClient } from "npm:@supabase/supabase-js@2";
import {
  buildMinecraftHandoffPayload,
  hasMinecraftEntitlement,
  minecraftProviderErrorCategory,
  normalizeMinecraftProfile,
} from "./policy.mjs";
const corsHeaders = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers":
    "authorization, apikey, x-client-info, content-type",
  "Access-Control-Allow-Methods": "POST, OPTIONS",
  "Cache-Control": "no-store",
};

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      ...corsHeaders,
      "Content-Type": "application/json; charset=utf-8",
    },
  });
}

const hex = (bytes: Uint8Array) =>
  Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
const sha256 = async (value: string) =>
  hex(
    new Uint8Array(
      await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value)),
    ),
  );
const randomHex = (length: number) =>
  hex(crypto.getRandomValues(new Uint8Array(length)));
const b64 = (bytes: Uint8Array) => btoa(String.fromCharCode(...bytes));

async function postJson(
  url: string,
  body: unknown,
  headers: Record<string, string> = {},
) {
  const response = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json", ...headers },
    body: JSON.stringify(body),
  });
  const text = await response.text();
  let data: Record<string, any> = {};
  try {
    data = JSON.parse(text);
  } catch {
    /* Report a generic upstream error below. */
  }
  if (!response.ok)
    throw new Error(
      `UPSTREAM_${response.status}:${data?.XErr ?? data?.error ?? "request failed"}`,
    );
  return data;
}

async function encryptPayload(value: unknown, secret: string): Promise<string> {
  const keyBytes = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(secret),
  );
  const key = await crypto.subtle.importKey("raw", keyBytes, "AES-GCM", false, [
    "encrypt",
  ]);
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const clear = new TextEncoder().encode(JSON.stringify(value));
  const cipher = new Uint8Array(
    await crypto.subtle.encrypt({ name: "AES-GCM", iv }, key, clear),
  );
  return `${b64(iv)}.${b64(cipher)}`;
}

function safeError(error: unknown): Response {
  const message = error instanceof Error ? error.message : "unknown";
  if (/ownership|entitlement|profile not owned/i.test(message))
    return json(
      {
        error:
          "Этот Microsoft-аккаунт не имеет Minecraft Java Edition или профиля Minecraft.",
      },
      403,
    );
  if (
    message.startsWith("UPSTREAM_401:") ||
    message.startsWith("UPSTREAM_403:")
  ) {
    return json(
      {
        error:
          "Microsoft или Xbox не разрешили вход. Проверьте Xbox-профиль и настройки конфиденциальности аккаунта.",
      },
      403,
    );
  }
  return json(
    { error: "Не удалось завершить вход Microsoft. Попробуйте снова." },
    502,
  );
}

Deno.serve(async (request: Request) => {
  if (request.method === "OPTIONS")
    return new Response("ok", { headers: corsHeaders });
  if (request.method !== "GET")
    return json({ error: "Method not allowed" }, 405);
  const url = Deno.env.get("SUPABASE_URL");
  const serviceKey = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY");
  const clientId = Deno.env.get("MICROSOFT_CLIENT_ID");
  const clientSecret = Deno.env.get("MICROSOFT_CLIENT_SECRET");
  const redirectUri = Deno.env.get("MICROSOFT_REDIRECT_URI");
  const returnUrl = Deno.env.get("MICROSOFT_RETURN_URL");
  const encryptionKey = Deno.env.get("MINECRAFT_HANDOFF_ENCRYPTION_KEY");
  if (!url || !serviceKey || !returnUrl)
    return json({ error: "Server callback is not configured" }, 503);
  const redirectBack = (
    key: "minecraft_handoff" | "minecraft_error",
    value: string,
  ) => {
    const destination = new URL(returnUrl);
    destination.searchParams.set(key, value);
    return Response.redirect(destination.toString(), 303);
  };
  if (
    !clientId ||
    !clientSecret ||
    !redirectUri ||
    !encryptionKey ||
    encryptionKey.length < 32
  )
    return redirectBack("minecraft_error", "not_configured");
  const callback = new URL(request.url);
  const code = callback.searchParams.get("code");
  const stateValue = callback.searchParams.get("state");
  const providerError = callback.searchParams.get("error");
  if (providerError)
    return redirectBack(
      "minecraft_error",
      minecraftProviderErrorCategory(providerError),
    );
  if (
    !code ||
    code.length > 4096 ||
    !stateValue ||
    !/^[a-f0-9]{64}$/.test(stateValue)
  )
    return redirectBack("minecraft_error", "invalid_response");

  const admin = createClient(url, serviceKey, {
    auth: { persistSession: false, autoRefreshToken: false },
  });
  const now = new Date().toISOString();
  const stateHash = await sha256(stateValue);
  const { data: state, error: stateError } = await admin
    .from("minecraft_oauth_states")
    .update({ consumed_at: now })
    .eq("state_hash", stateHash)
    .is("consumed_at", null)
    .gt("expires_at", now)
    .select("code_verifier,user_id")
    .maybeSingle();
  if (stateError) return redirectBack("minecraft_error", "state_check_failed");
  if (!state) return redirectBack("minecraft_error", "expired");

  try {
    const tokenBody = new URLSearchParams({
      client_id: clientId,
      client_secret: clientSecret,
      grant_type: "authorization_code",
      code,
      redirect_uri: redirectUri,
      code_verifier: state.code_verifier,
      scope: "xboxlive.signin",
    });
    const tokenResponse = await fetch(
      "https://login.microsoftonline.com/consumers/oauth2/v2.0/token",
      {
        method: "POST",
        headers: { "content-type": "application/x-www-form-urlencoded" },
        body: tokenBody,
      },
    );
    const tokenText = await tokenResponse.text();
    let msToken: Record<string, any> = {};
    try {
      msToken = JSON.parse(tokenText);
    } catch {
      /* generic response below */
    }
    if (!tokenResponse.ok || typeof msToken.access_token !== "string")
      throw new Error(
        `UPSTREAM_${tokenResponse.status}:${msToken.error ?? "token exchange failed"}`,
      );

    const xbox = await postJson(
      "https://user.auth.xboxlive.com/user/authenticate",
      {
        Properties: {
          AuthMethod: "RPS",
          SiteName: "user.auth.xboxlive.com",
          RpsTicket: `d=${msToken.access_token}`,
        },
        RelyingParty: "http://auth.xboxlive.com",
        TokenType: "JWT",
      },
      { "x-xbl-contract-version": "1" },
    );
    if (typeof xbox.Token !== "string")
      throw new Error("UPSTREAM_502:xbox user token missing");

    const xsts = await postJson(
      "https://xsts.auth.xboxlive.com/xsts/authorize",
      {
        Properties: { SandboxId: "RETAIL", UserTokens: [xbox.Token] },
        RelyingParty: "rp://api.minecraftservices.com/",
        TokenType: "JWT",
      },
      { "x-xbl-contract-version": "1" },
    );
    const xui = xsts?.DisplayClaims?.xui?.[0];
    if (
      typeof xsts.Token !== "string" ||
      typeof xui?.uhs !== "string" ||
      typeof xui?.xid !== "string"
    )
      throw new Error("UPSTREAM_502:xbox identity missing");
    const mc = await postJson(
      "https://api.minecraftservices.com/authentication/login_with_xbox",
      { identityToken: `XBL3.0 x=${xui.uhs};${xsts.Token}` },
    );
    if (typeof mc.access_token !== "string")
      throw new Error("UPSTREAM_502:minecraft access token missing");
    const mcHeaders = { authorization: `Bearer ${mc.access_token}` };
    const entitlementResponse = await fetch(
      "https://api.minecraftservices.com/entitlements/mcstore",
      { headers: mcHeaders },
    );
    const entitlementText = await entitlementResponse.text();
    let entitlements: Record<string, any> = {};
    try {
      entitlements = JSON.parse(entitlementText);
    } catch {
      /* The response is validated below. */
    }
    if (!entitlementResponse.ok || !hasMinecraftEntitlement(entitlements))
      throw new Error(
        `UPSTREAM_${entitlementResponse.status || 403}:ownership entitlement missing`,
      );
    const profileResponse = await fetch(
      "https://api.minecraftservices.com/minecraft/profile",
      { headers: mcHeaders },
    );
    const profileText = await profileResponse.text();
    let profile: Record<string, any> = {};
    try {
      profile = JSON.parse(profileText);
    } catch {
      /* generic response below */
    }
    if (!profileResponse.ok)
      throw new Error(`UPSTREAM_${profileResponse.status}:profile not owned`);
    const minecraftProfile = normalizeMinecraftProfile(profile);
    if (!minecraftProfile)
      throw new Error("UPSTREAM_502:invalid Minecraft profile");

    const { data: existingXuid, error: existingXuidError } = await admin
      .from("minecraft_account_links")
      .select("user_id")
      .eq("xuid", xui.xid)
      .maybeSingle();
    if (existingXuidError)
      throw new Error(
        "UPSTREAM_500:could not verify Microsoft account ownership",
      );
    if (existingXuid && existingXuid.user_id !== state.user_id)
      return redirectBack("minecraft_error", "already_linked_elsewhere");
    const { data: currentLink, error: currentLinkError } = await admin
      .from("minecraft_account_links")
      .select("xuid")
      .eq("user_id", state.user_id)
      .maybeSingle();
    if (currentLinkError)
      throw new Error("UPSTREAM_500:could not check current Microsoft link");
    if (currentLink && currentLink.xuid !== xui.xid)
      return redirectBack("minecraft_error", "another_account_linked");

    const profileId = minecraftProfile.id;
    const minecraftUuid = minecraftProfile.uuid;
    const accountId = `microsoft-${minecraftUuid}`;
    const expiresIn = mc.expires_in ?? 86_400;
    if (!Number.isSafeInteger(expiresIn) || expiresIn <= 0)
      throw new Error("UPSTREAM_502:invalid Minecraft token expiry");
    const expiresAt = Math.floor(Date.now() / 1000) + expiresIn;
    if (!Number.isSafeInteger(expiresAt))
      throw new Error("UPSTREAM_502:invalid Minecraft token expiry");
    const createdAt = new Date().toISOString();
    const payload = buildMinecraftHandoffPayload({
      profile: minecraftProfile,
      xuid: xui.xid,
      clientId,
      accessToken: mc.access_token,
      expiresAt,
      createdAt,
    });
    const encryptedPayload = await encryptPayload(payload, encryptionKey);
    const code = randomHex(32);
    const { error: linkError } = await admin
      .from("minecraft_account_links")
      .upsert(
        {
          user_id: state.user_id,
          xuid: xui.xid,
          minecraft_uuid: minecraftUuid,
          minecraft_username: minecraftProfile.name,
        },
        { onConflict: "user_id" },
      );
    if (linkError)
      throw new Error("UPSTREAM_500:could not save linked identity");
    const { error: handoffError } = await admin
      .from("minecraft_auth_handoffs")
      .insert({
        user_id: state.user_id,
        code_hash: await sha256(code),
        encrypted_payload: encryptedPayload,
        expires_at: new Date(Date.now() + 60_000).toISOString(),
      });
    if (handoffError)
      throw new Error("UPSTREAM_500:could not create secure handoff");
    return redirectBack("minecraft_handoff", code);
  } catch (error) {
    const failed = safeError(error);
    const category =
      failed.status === 403 ? "minecraft_unavailable" : "upstream_failed";
    return redirectBack("minecraft_error", category);
  }
});
