import test from "node:test";
import assert from "node:assert/strict";
import { isAllowedLoginOrigin, signInByNickname } from "./signin.mjs";

function createAdmin({ profile, user, profileError, userError } = {}) {
  const calls = [];
  const admin = {
    from(table) {
      assert.equal(table, "minlauncher_nicknames");
      const query = {
        select(columns) {
          assert.equal(columns, "user_id");
          return query;
        },
        eq(column, value) {
          calls.push([column, value]);
          return query;
        },
        async maybeSingle() {
          return { data: profile ?? null, error: profileError ?? null };
        },
      };
      return query;
    },
    auth: {
      admin: {
        async getUserById(id) {
          calls.push(["user_id", id]);
          return { data: { user: user ?? null }, error: userError ?? null };
        },
      },
    },
  };
  return { admin, calls };
}

test("nickname sign-in resolves the registered account and returns only session tokens", async () => {
  const { admin, calls } = createAdmin({
    profile: { user_id: "user-1" },
    user: { email: "player@example.com" },
  });
  let request;
  const result = await signInByNickname({
    admin,
    supabaseUrl: "https://project.supabase.co",
    anonKey: "public-key",
    nickname: "MiN_Player7",
    password: "correct horse battery",
    fetcher: async (url, options) => {
      request = { url, options, body: JSON.parse(options.body) };
      return new Response(
        JSON.stringify({
          access_token: "access",
          refresh_token: "refresh",
          token_type: "bearer",
          expires_in: 3600,
          user: { email: "player@example.com" },
        }),
        { status: 200 },
      );
    },
  });

  assert.deepEqual(calls.slice(0, 2), [
    ["nickname_normalized", "min_player7"],
    ["owner_kind", "registered"],
  ]);
  assert.equal(
    request.url,
    "https://project.supabase.co/auth/v1/token?grant_type=password",
  );
  assert.deepEqual(request.body, {
    email: "player@example.com",
    password: "correct horse battery",
  });
  assert.deepEqual(result, {
    status: 200,
    body: {
      access_token: "access",
      refresh_token: "refresh",
      token_type: "bearer",
      expires_in: 3600,
    },
  });
});

test("unknown nickname and wrong password have the same response", async () => {
  const unknown = createAdmin();
  const authBodies = [];
  const fetcher = async (_url, options) => {
    authBodies.push(JSON.parse(options.body));
    return new Response(
      JSON.stringify({ message: "Invalid login credentials" }),
      { status: 400 },
    );
  };
  const result = await signInByNickname({
    admin: unknown.admin,
    supabaseUrl: "https://project.supabase.co",
    anonKey: "public-key",
    nickname: "Missing_1",
    password: "some password",
    fetcher,
  });
  const invalidPassword = createAdmin({
    profile: { user_id: "user-2" },
    user: { email: "player@example.com" },
  });
  const invalidResult = await signInByNickname({
    admin: invalidPassword.admin,
    supabaseUrl: "https://project.supabase.co",
    anonKey: "public-key",
    nickname: "Existing_1",
    password: "some password",
    fetcher,
  });

  assert.deepEqual(result, invalidResult);
  assert.equal(result.status, 401);
  assert.deepEqual(result.body, { error: "Неверный ник или пароль." });
  assert.match(
    authBodies[0].email,
    /^unknown-[a-f0-9]+@login\.minlauncher\.invalid$/,
  );
  assert.equal(authBodies[0].password, authBodies[1].password);
});

test("nickname sign-in never returns upstream authentication details", async () => {
  const { admin } = createAdmin({
    profile: { user_id: "user-1" },
    user: { email: "player@example.com" },
  });
  const result = await signInByNickname({
    admin,
    supabaseUrl: "https://project.supabase.co",
    anonKey: "public-key",
    nickname: "Player_1",
    password: "wrong",
    fetcher: async () =>
      new Response(JSON.stringify({ message: "email not confirmed" }), {
        status: 400,
      }),
  });

  assert.deepEqual(result, {
    status: 401,
    body: { error: "Неверный ник или пароль." },
  });
});

test("nickname login accepts only the production site and local development origins", () => {
  assert.equal(isAllowedLoginOrigin("https://minlauncher.pages.dev"), true);
  assert.equal(isAllowedLoginOrigin("http://127.0.0.1:4173"), true);
  assert.equal(isAllowedLoginOrigin("https://attacker.example"), false);
});

test("nickname sign-in rejects a password over the UTF-8 byte limit without sending it", async () => {
  const { admin } = createAdmin();
  let fetched = false;
  const result = await signInByNickname({
    admin,
    supabaseUrl: "https://project.supabase.co",
    anonKey: "public-key",
    nickname: "Player_1",
    password: "я".repeat(37),
    fetcher: async () => {
      fetched = true;
      return new Response("{}", { status: 200 });
    },
  });

  assert.equal(fetched, false);
  assert.deepEqual(result, {
    status: 401,
    body: { error: "Неверный ник или пароль." },
  });
});
