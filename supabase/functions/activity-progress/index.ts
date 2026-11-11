import { createClient } from "npm:@supabase/supabase-js@2";
import { createActivityProgressHandler } from "./handler.mjs";

const supabaseUrl = Deno.env.get("SUPABASE_URL");
const anonKey = Deno.env.get("SUPABASE_ANON_KEY");
const serviceRoleKey = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY");

if (!supabaseUrl || !anonKey || !serviceRoleKey) {
  Deno.serve(
    () =>
      new Response(
        JSON.stringify({ error: "Activity service is not configured" }),
        {
          status: 503,
          headers: {
            "Content-Type": "application/json; charset=utf-8",
            "Cache-Control": "no-store",
          },
        },
      ),
  );
} else {
  const admin = createClient(supabaseUrl, serviceRoleKey, {
    auth: { persistSession: false, autoRefreshToken: false },
  });
  const handler = createActivityProgressHandler({
    async getUser(accessToken) {
      const userClient = createClient(supabaseUrl, anonKey, {
        global: { headers: { Authorization: `Bearer ${accessToken}` } },
        auth: { persistSession: false, autoRefreshToken: false },
      });
      const {
        data: { user },
        error,
      } = await userClient.auth.getUser();
      if (!error) return user?.id ?? null;
      const status = (error as { status?: number }).status;
      if (status === 401 || status === 403) return null;
      throw new Error("Supabase auth verification failed");
    },
    async rpc(name, args) {
      return await admin.rpc(name, args);
    },
    log(event, code) {
      console.warn(event, code);
    },
  });

  Deno.serve(handler);
}
