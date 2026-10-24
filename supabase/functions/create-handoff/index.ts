import { createClient } from "npm:@supabase/supabase-js@2";
import { corsHeaders, json } from "../_shared/http.ts";

Deno.serve(async (request: Request) => {
  if (request.method === "OPTIONS") return new Response("ok", { headers: corsHeaders });
  if (request.method !== "POST") return json({ error: "Method not allowed" }, 405);

  const authorization = request.headers.get("Authorization");
  const url = Deno.env.get("SUPABASE_URL");
  const anonKey = Deno.env.get("SUPABASE_ANON_KEY");
  const serviceKey = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY");
  if (!authorization?.startsWith("Bearer ") || !url || !anonKey || !serviceKey) {
    return json({ error: "Authentication or server configuration is missing" }, 401);
  }

  const userClient = createClient(url, anonKey, {
    global: { headers: { Authorization: authorization } },
    auth: { persistSession: false, autoRefreshToken: false },
  });
  const { data: { user }, error: userError } = await userClient.auth.getUser();
  if (userError || !user) return json({ error: "Sign in to create a launcher link" }, 401);

  const admin = createClient(url, serviceKey, {
    auth: { persistSession: false, autoRefreshToken: false },
  });
  const now = new Date();
  const expiresAt = new Date(now.getTime() + 60_000).toISOString();
  const cleanupBefore = new Date(now.getTime() - 5 * 60_000).toISOString();
  const { error: cleanupError } = await admin.from("minlauncher_auth_handoffs")
    .delete().lt("expires_at", cleanupBefore);
  if (cleanupError) return json({ error: "Could not prepare a launcher link" }, 500);
  const { error: replaceError } = await admin.from("minlauncher_auth_handoffs").delete()
    .eq("user_id", user.id).is("consumed_at", null).gt("expires_at", now.toISOString());
  if (replaceError) return json({ error: "Could not prepare a launcher link" }, 500);

  const bytes = crypto.getRandomValues(new Uint8Array(32));
  const code = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(code));
  const codeHash = Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
  const { error: insertError } = await admin.from("minlauncher_auth_handoffs").insert({
    user_id: user.id,
    code_hash: codeHash,
    expires_at: expiresAt,
  });
  if (insertError) return json({ error: "Could not create a launcher link" }, 500);
  return json({ code, expires_at: expiresAt });
});
