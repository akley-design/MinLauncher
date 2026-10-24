import assert from "node:assert/strict";
import { test } from "node:test";

import { isSensitivePath, nextPatchVersion } from "./publish-release-utils.mjs";

test("the next release increments only the patch number", () => {
  assert.equal(nextPatchVersion("0.1.9"), "0.1.10");
  assert.equal(nextPatchVersion("1.4.2"), "1.4.3");
});

test("release versions must be plain three-part semantic versions", () => {
  assert.throws(() => nextPatchVersion("v1.2.3"), /X.Y.Z/);
  assert.throws(() => nextPatchVersion("1.2"), /X.Y.Z/);
});

test("the publisher rejects common local secret files", () => {
  assert.equal(isSensitivePath(".env.local"), true);
  assert.equal(isSensitivePath("crates/ml-desktop/signing.key"), true);
  assert.equal(isSensitivePath("apps/site/.env.example"), false);
  assert.equal(isSensitivePath("README.md"), false);
});
