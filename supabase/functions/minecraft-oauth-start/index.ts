import { createClient } from "npm:@supabase/supabase-js@2";
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
const randomHex = (length: number) =>
  hex(crypto.getRandomValues(new Uint8Array(length)));
const sha256 = async (value: string) =>
  hex(
    new Uint8Array(
      await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value)),
    ),
  );
const base64url = (bytes: Uint8Array) =>
  btoa(String.fromCharCode(...bytes))
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/g, "");

Deno.serve(async (request: Request) => {
  if (request.method === "OPTIONS")
    return new Response("ok", { headers: corsHeaders });
  if (request.method !== "POST")
    return json({ error: "Method not allowed" }, 405);

  const authorization = request.headers.get("Authorization");
  const url = Deno.env.get("SUPABASE_URL");
  const anonKey = Deno.env.get("SUPABASE_ANON_KEY");
  const serviceKey = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY");
  const clientId = Deno.env.get("MICROSOFT_CLIENT_ID");
  const redirectUri = Deno.env.get("MICROSOFT_REDIRECT_URI");
  if (!authorization?.startsWith("Bearer ") || !url || !anonKey || !serviceKey)
    return json({ error: "Sign in required" }, 401);
  if (!clientId || !redirectUri)
    return json(
      { error: "Microsoft sign-in is not configured on the server" },
      503,
    );

  const userClient = createClient(url, anonKey, {
    global: { headers: { Authorization: authorization } },
    auth: { persistSession: false, autoRefreshToken: false },
  });
  const {
    data: { user },
    error,
  } = await userClient.auth.getUser();
  if (error || !user)
    return json({ error: "Sign in to MinLauncher first" }, 401);

  const state = randomHex(32);
  const verifier = randomHex(48);
  const challenge = base64url(
    new Uint8Array(
      await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier)),
    ),
  );
  const admin = createClient(url, serviceKey, {
    auth: { persistSession: false, autoRefreshToken: false },
  });
  const now = new Date();
  const { error: cleanupError } = await admin
    .from("minecraft_oauth_states")
    .delete()
    .lt("expires_at", new Date(now.getTime() - 5 * 60_000).toISOString());
  if (cleanupError)
    return json({ error: "Could not prepare Microsoft sign-in" }, 500);
  const { error: insertError } = await admin
    .from("minecraft_oauth_states")
    .insert({
      state_hash: await sha256(state),
      user_id: user.id,
      code_verifier: verifier,
      expires_at: new Date(now.getTime() + 10 * 60_000).toISOString(),
    });
  if (insertError)
    return json({ error: "Could not prepare Microsoft sign-in" }, 500);

  const authorize = new URL(
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize",
  );
  authorize.search = new URLSearchParams({
    client_id: clientId,
    response_type: "code",
    response_mode: "query",
    redirect_uri: redirectUri,
    scope: "xboxlive.signin",
    state,
    code_challenge: challenge,
    code_challenge_method: "S256",
    prompt: "select_account",
  }).toString();
  return json({ authorize_url: authorize.toString() });
});
