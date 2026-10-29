import assert from "node:assert/strict";
import test from "node:test";
import { completeHandoff } from "./handoff.mjs";

function createAdmin({ failLink = false } = {}) {
  const events = [];
  const state = { consumed: false };
  return {
    events,
    from() {
      let operation = "lookup";
      const query = {
        select() {
          return query;
        },
        update() {
          operation = "consume";
          events.push("consume");
          return query;
        },
        eq() {
          return query;
        },
        is() {
          return query;
        },
        gt() {
          return query;
        },
        async maybeSingle() {
          if (operation === "lookup") {
            events.push("lookup");
            return { data: { user_id: "user-1" }, error: null };
          }
          if (state.consumed) return { data: null, error: null };
          state.consumed = true;
          return { data: { code_hash: "hash" }, error: null };
        },
      };
      return query;
    },
    auth: {
      admin: {
        async getUserById() {
          events.push("user");
          return {
            data: { user: { email: "player@example.com" } },
            error: null,
          };
        },
        async generateLink() {
          events.push("link");
          return failLink
            ? { data: null, error: new Error("temporary failure") }
            : {
                data: { properties: { hashed_token: "token" } },
                error: null,
              };
        },
      },
    },
  };
}

test("a failed magic-link request leaves the handoff unused", async () => {
  const admin = createAdmin({ failLink: true });
  const result = await completeHandoff(admin, "hash");

  assert.equal(result.status, 500);
  assert.deepEqual(admin.events, ["lookup", "user", "link"]);
});

test("only one concurrent request consumes a handoff", async () => {
  const admin = createAdmin();
  const results = await Promise.all([
    completeHandoff(admin, "hash"),
    completeHandoff(admin, "hash"),
  ]);

  assert.equal(results.filter((result) => result.status === 200).length, 1);
  assert.equal(results.filter((result) => result.status === 401).length, 1);
  assert.equal(admin.events.filter((event) => event === "consume").length, 2);
});
