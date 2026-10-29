import { createClient } from "npm:@supabase/supabase-js@2";
import { corsHeaders, json } from "../_shared/http.ts";
import { completeHandoff } from "./handoff.mjs";

Deno.serve(async (request: Request) => {
  if (request.method === "OPTIONS")
    return new Response("ok", { headers: corsHeaders });
  if (request.method !== "POST")
    return json({ error: "Method not allowed" }, 405);
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
    return json(
      {
        error:
          "This launcher link is invalid or expired. Create a new link on the website.",
      },
      400,
    );
  }

  const url = Deno.env.get("SUPABASE_URL");
  const serviceKey = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY");
  if (!url || !serviceKey)
    return json({ error: "Server configuration is missing" }, 500);
  const admin = createClient(url, serviceKey, {
    auth: { persistSession: false, autoRefreshToken: false },
  });
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(code),
  );
  const codeHash = Array.from(new Uint8Array(digest), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
  const result = await completeHandoff(admin, codeHash);
  if (result.status !== 200)
    return json({ error: result.error }, result.status);
  return json({ token_hash: result.tokenHash });
});
