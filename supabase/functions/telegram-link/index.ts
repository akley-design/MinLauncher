import { createClient } from "npm:@supabase/supabase-js@2";
import { createTelegramLinkHandler } from "./handler.mjs";

const supabaseUrl = Deno.env.get("SUPABASE_URL");
const anonKey = Deno.env.get("SUPABASE_ANON_KEY");
const serviceRoleKey = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY");
const botSecret = Deno.env.get("MINLAUNCHER_BOT_SECRET");

if (!supabaseUrl || !anonKey || !serviceRoleKey || !botSecret) {
  Deno.serve((request) => {
    const headers = {
      "Access-Control-Allow-Origin": "*",
      "Access-Control-Allow-Headers": "authorization, apikey, x-client-info, content-type, x-minlauncher-bot-secret",
      "Access-Control-Allow-Methods": "POST, OPTIONS",
      "Cache-Control": "no-store",
      "Content-Type": "application/json; charset=utf-8",
    };
    if (request.method === "OPTIONS") {
      return new Response(JSON.stringify({ ok: true }), { headers });
    }
    return new Response(JSON.stringify({ error: "service_unavailable" }), {
      status: 503,
      headers,
    });
  });
} else {
  const admin = createClient(supabaseUrl, serviceRoleKey, {
    auth: { persistSession: false, autoRefreshToken: false },
  });
  const handler = createTelegramLinkHandler({
    botSecret,
    async getUser(accessToken) {
      const userClient = createClient(supabaseUrl, anonKey, {
        auth: { persistSession: false, autoRefreshToken: false },
      });
      const {
        data: { user },
        error,
      } = await userClient.auth.getUser(accessToken);
      if (!error) return user?.id ?? null;
      const status = (error as { status?: number }).status;
      if (status === 401 || status === 403) return null;
      throw new Error("Supabase auth verification failed");
    },
    store: {
      async findLinkByUserId(userId) {
        const { data, error } = await admin
          .from("minlauncher_telegram_links")
          .select("user_id")
          .eq("user_id", userId)
          .maybeSingle();
        if (error) throw new Error("Telegram link lookup failed");
        return data;
      },
      async createChallenge({ userId, tokenHash, expiresAt }) {
        const { error } = await admin
          .from("minlauncher_telegram_link_challenges")
          .upsert(
            {
              user_id: userId,
              token_hash: tokenHash,
              created_at: new Date().toISOString(),
              expires_at: expiresAt,
              consumed_at: null,
            },
            { onConflict: "user_id" },
          );
        if (error) throw new Error("Telegram challenge creation failed");
      },
      async redeemChallenge({ tokenHash, telegramUserId }) {
        const { data, error } = await admin.rpc("minlauncher_redeem_telegram_link", {
          p_token_hash: tokenHash,
          p_telegram_user_id: telegramUserId,
        });
        if (error) throw new Error("Telegram challenge redemption failed");
        return { status: data };
      },
      async findUserIdByTelegramId(telegramUserId) {
        const { data, error } = await admin
          .from("minlauncher_telegram_links")
          .select("user_id")
          .eq("telegram_user_id", telegramUserId)
          .maybeSingle();
        if (error) throw new Error("Telegram progress link lookup failed");
        return data?.user_id ?? null;
      },
      async getActivity(userId) {
        const { data, error } = await admin.rpc("minlauncher_activity_get_progress", {
          p_user_id: userId,
        });
        if (error) throw new Error("Activity progress lookup failed");
        return data;
      },
      async unlinkByUserId(userId) {
        const { error } = await admin.rpc("minlauncher_unlink_telegram", {
          p_user_id: userId,
          p_telegram_user_id: null,
        });
        if (error) throw new Error("Telegram unlink failed");
      },
      async unlinkByTelegramId(telegramUserId) {
        const { error } = await admin.rpc("minlauncher_unlink_telegram", {
          p_user_id: null,
          p_telegram_user_id: telegramUserId,
        });
        if (error) throw new Error("Telegram unlink failed");
      },
    },
    log(event, code) {
      console.warn(event, code);
    },
  });

  Deno.serve(handler);
}