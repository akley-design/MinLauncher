import { createClient } from "@supabase/supabase-js";
import { invoke, isTauri } from "@tauri-apps/api/core";

const url = import.meta.env.VITE_SUPABASE_URL?.trim();
const key = import.meta.env.VITE_SUPABASE_ANON_KEY?.trim();

const credentialVault = {
  getItem: (name: string) =>
    isTauri() ? invoke<string | null>("identity_storage_get", { key: name }) : Promise.resolve(null),
  setItem: (name: string, value: string) =>
    isTauri() ? invoke<void>("identity_storage_set", { key: name, value }) : Promise.resolve(),
  removeItem: (name: string) =>
    isTauri() ? invoke<void>("identity_storage_remove", { key: name }) : Promise.resolve(),
};

export const launcherAuth = url && key
  ? createClient(url, key, {
      auth: {
        storage: credentialVault,
        flowType: "pkce",
        detectSessionInUrl: false,
        persistSession: true,
        autoRefreshToken: true,
      },
    })
  : null;

export function parseMinLauncherUrl(raw: string):
  | { kind: "oauth"; code: string }
  | { kind: "transfer"; code: string }
  | { kind: "error"; message: string }
  | null {
  try {
    const url = new URL(raw);
    if (url.protocol !== "minlauncher:" || url.hostname !== "auth") return null;
    const params = new URLSearchParams(url.search);
    const hash = new URLSearchParams(url.hash.replace(/^#/, ""));
    const message = params.get("error_description") ?? params.get("error") ??
      hash.get("error_description") ?? hash.get("error");
    if (message) return { kind: "error", message: message.slice(0, 240) };
    const code = params.get("code");
    if (url.pathname !== "/callback" && url.pathname !== "/transfer") return null;
    if (!code || code.length > 2048) return { kind: "error", message: "Ссылка входа повреждена или устарела." };
    if (url.pathname === "/callback") return { kind: "oauth", code };
    if (url.pathname === "/transfer") return { kind: "transfer", code };
    return null;
  } catch {
    return null;
  }
}
