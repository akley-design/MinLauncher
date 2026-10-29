import test from "node:test";
import assert from "node:assert/strict";
import { normalizeNickname } from "./nickname.mjs";

test("normalizes case for a single global nickname namespace", () => {
  assert.equal(normalizeNickname("MiN_Player7"), "min_player7");
});

test("accepts only 3 to 16 ASCII letters, digits, and underscores", () => {
  for (const name of ["Ab1", "abcdefghijklmnop", "a_b", "123"]) {
    assert.notEqual(normalizeNickname(name), null);
  }
  for (const name of [
    "ab",
    "abcdefghijklmnopq",
    "bad name",
    "имя",
    "a-b",
    "",
  ]) {
    assert.equal(normalizeNickname(name), null);
  }
});
