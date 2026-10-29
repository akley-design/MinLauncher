import { createClient } from "npm:@supabase/supabase-js@2";
import { consumeMinecraftHandoff } from "./handoff.mjs";
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
const fromB64 = (value: string) =>
  Uint8Array.from(atob(value), (character) => character.charCodeAt(0));

async function decryptPayload(value: string, secret: string): Promise<unknown> {
  const [ivText, cipherText] = value.split(".");
  if (!ivText || !cipherText) throw new Error("invalid encrypted payload");
  const keyBytes = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(secret),
  );
  const key = await crypto.subtle.importKey("raw", keyBytes, "AES-GCM", false, [
    "decrypt",
  ]);
  const clear = await crypto.subtle.decrypt(
    { name: "AES-GCM", iv: fromB64(ivText) },
    key,
    fromB64(cipherText),
  );
  return JSON.parse(new TextDecoder().decode(clear));
}

Deno.serve(async (request: Request) => {
  if (request.method === "OPTIONS")
    return new Response("ok", { headers: corsHeaders });
  if (request.method !== "POST")
    return json({ error: "Method not allowed" }, 405);
  const url = Deno.env.get("SUPABASE_URL");
  const serviceKey = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY");
  const encryptionKey = Deno.env.get("MINECRAFT_HANDOFF_ENCRYPTION_KEY");
  if (!url || !serviceKey || !encryptionKey || encryptionKey.length < 32)
    return json({ error: "Server configuration is missing" }, 500);
  const raw = await request.text();
  if (raw.length > 4096) return json({ error: "Invalid request" }, 413);
  let body: Record<string, unknown>;
  try {
    body = JSON.parse(raw);
  } catch {
    return json({ error: "Invalid request" }, 400);
  }
  if (typeof body.code !== "string" || !/^[a-f0-9]{64}$/.test(body.code))
    return json(
      {
        error:
          "Ссылка для Minecraft-входа повреждена или истекла. Создайте новую на сайте.",
      },
      400,
    );

  const admin = createClient(url, serviceKey, {
    auth: { persistSession: false, autoRefreshToken: false },
  });
  const handoff = await consumeMinecraftHandoff(admin, await sha256(body.code));
  if (handoff.kind === "error")
    return json({ error: "Не удалось завершить подключение Minecraft." }, 500);
  if (handoff.kind === "expired_or_used")
    return json(
      {
        error:
          "Ссылка для Minecraft-входа уже использована или истекла. Создайте новую на сайте.",
      },
      400,
    );
  try {
    return json({
      payload: await decryptPayload(handoff.encryptedPayload, encryptionKey),
    });
  } catch {
    return json({ error: "Не удалось расшифровать передачу аккаунта." }, 500);
  }
});
