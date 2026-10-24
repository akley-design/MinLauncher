import { createClient } from "npm:@supabase/supabase-js@2";
import { corsHeaders, json } from "../_shared/http.ts";

Deno.serve(async (request: Request) => {
  if (request.method === "OPTIONS") return new Response("ok", { headers: corsHeaders });
  if (request.method !== "POST") return json({ error: "Method not allowed" }, 405);
  const rawBody = await request.text();
  if (rawBody.length > 4096) {
    return json({ error: "Invalid request" }, 413);
  }

  let code: unknown;
  try {
    ({ code } = JSON.parse(rawBody));
  } catch {
    return json({ error: "Invalid request" }, 400);
  }
  if (typeof code !== "string" || !/^[a-f0-9]{64}$/.test(code)) {
    return json({ error: "This launcher link is invalid or expired. Create a new link on the website." }, 400);
  }

  const url = Deno.env.get("SUPABASE_URL");
  const serviceKey = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY");
  if (!url || !serviceKey) return json({ error: "Server configuration is missing" }, 500);
  const admin = createClient(url, serviceKey, {
    auth: { persistSession: false, autoRefreshToken: false },
  });
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(code));
  const codeHash = Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
  const now = new Date().toISOString();
  const { data: handoff, error: consumeError } = await admin
    .from("minlauncher_auth_handoffs")
    .update({ consumed_at: now })
    .eq("code_hash", codeHash)
    .is("consumed_at", null)
    .gt("expires_at", now)
    .select("user_id")
    .maybeSingle();
  if (consumeError || !handoff) {
    return json({ error: "This launcher link was already used or has expired. Create a new link on the website." }, 401);
  }

  const { data: userData, error: userError } = await admin.auth.admin.getUserById(handoff.user_id);
  const email = userData.user?.email;
  if (userError || !email) return json({ error: "This account has no email available for secure sign-in" }, 400);

  const { data: linkData, error: linkError } = await admin.auth.admin.generateLink({
    type: "magiclink",
    email,
  });
  const tokenHash = linkData.properties?.hashed_token;
  if (linkError || !tokenHash) return json({ error: "Could not complete secure sign-in" }, 500);
  return json({ token_hash: tokenHash });
});
