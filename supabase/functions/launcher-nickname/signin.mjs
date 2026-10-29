import { normalizeNickname } from "./nickname.mjs";

const invalidCredentials = {
  status: 401,
  body: { error: "Неверный ник или пароль." },
};
const allowedLoginOrigins = new Set([
  "https://minlauncher.pages.dev",
  "http://localhost:4173",
  "http://127.0.0.1:4173",
  "http://localhost:5173",
  "http://127.0.0.1:5173",
]);

export function isAllowedLoginOrigin(origin) {
  return allowedLoginOrigins.has(origin);
}

async function digest(value) {
  const bytes = new TextEncoder().encode(value);
  const hash = await crypto.subtle.digest("SHA-256", bytes);
  return Array.from(new Uint8Array(hash), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
}

export async function signInByNickname({
  admin,
  supabaseUrl,
  anonKey,
  nickname,
  password,
  fetcher = fetch,
}) {
  const normalized = normalizeNickname(nickname);
  if (
    !normalized ||
    typeof password !== "string" ||
    password.length < 1 ||
    new TextEncoder().encode(password).length > 72
  ) {
    return invalidCredentials;
  }

  const { data: profile, error: profileError } = await admin
    .from("minlauncher_nicknames")
    .select("user_id")
    .eq("nickname_normalized", normalized)
    .eq("owner_kind", "registered")
    .maybeSingle();
  if (profileError) throw new Error("Could not resolve nickname login");

  let email = null;
  if (profile?.user_id) {
    const { data, error } = await admin.auth.admin.getUserById(profile.user_id);
    if (error) throw new Error("Could not resolve nickname login");
    email = data?.user?.email ?? null;
  }

  // Unknown nicks still reach GoTrue so clients cannot distinguish them by
  // response shape or by skipping its per-IP password-grant throttles.
  if (!email)
    email = `unknown-${await digest(normalized)}@login.minlauncher.invalid`;
  let authResponse;
  try {
    authResponse = await fetcher(
      `${supabaseUrl.replace(/\/$/, "")}/auth/v1/token?grant_type=password`,
      {
        method: "POST",
        headers: { apikey: anonKey, "content-type": "application/json" },
        body: JSON.stringify({ email, password }),
      },
    );
  } catch {
    return {
      status: 503,
      body: { error: "Вход временно недоступен. Попробуйте позже." },
    };
  }

  if (!authResponse.ok) return invalidCredentials;
  let session;
  try {
    session = await authResponse.json();
  } catch {
    return {
      status: 503,
      body: { error: "Вход временно недоступен. Попробуйте позже." },
    };
  }
  if (
    typeof session?.access_token !== "string" ||
    typeof session?.refresh_token !== "string"
  ) {
    return {
      status: 503,
      body: { error: "Вход временно недоступен. Попробуйте позже." },
    };
  }

  return {
    status: 200,
    body: {
      access_token: session.access_token,
      refresh_token: session.refresh_token,
      token_type: session.token_type,
      expires_in: session.expires_in,
    },
  };
}
