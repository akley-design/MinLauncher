// The test of the update manifest.
//
// The manifest is the only thing that stands between a player and a file
// that nobody signed, so the two things it must get right are checked here:
// the signature of an artifact is the content of its `.sig` file, and the
// keys of the manifest are the ones the updater looks for.

import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { buildManifest, collectArtifacts } from "./latest-json.mjs";

/** A bundle with one installer of each kind, and the signature of each. */
function bundle() {
  const root = mkdtempSync(join(tmpdir(), "minlaucher-bundle-"));
  mkdirSync(join(root, "nsis"));
  mkdirSync(join(root, "msi"));
  writeFileSync(
    join(root, "nsis", "MinLauncher_0.1.0_x64-setup.exe"),
    "installer",
  );
  writeFileSync(
    join(root, "nsis", "MinLauncher_0.1.0_x64-setup.exe.sig"),
    "nsis-signature",
  );
  writeFileSync(
    join(root, "msi", "MinLauncher_0.1.0_x64_en-US.msi"),
    "installer",
  );
  writeFileSync(
    join(root, "msi", "MinLauncher_0.1.0_x64_en-US.msi.sig"),
    "msi-signature",
  );
  return root;
}

test("the manifest names the installer of every kind with its signature", () => {
  const artifacts = collectArtifacts(bundle());
  const manifest = buildManifest({
    version: "0.1.0",
    notes: "the first release",
    pubDate: "2026-10-20T00:00:00.000Z",
    baseUrl: "https://example.invalid/releases/v0.1.0",
    artifacts,
  });

  assert.equal(manifest.version, "0.1.0");
  assert.equal(manifest.notes, "the first release");
  assert.equal(manifest.pub_date, "2026-10-20T00:00:00.000Z");
  assert.deepEqual(manifest.platforms["windows-x86_64-nsis"], {
    signature: "nsis-signature",
    url: "https://example.invalid/releases/v0.1.0/MinLauncher_0.1.0_x64-setup.exe",
  });
  assert.deepEqual(manifest.platforms["windows-x86_64-msi"], {
    signature: "msi-signature",
    url: "https://example.invalid/releases/v0.1.0/MinLauncher_0.1.0_x64_en-US.msi",
  });
  // The fallback of the clients that know nothing about installer kinds.
  assert.deepEqual(
    manifest.platforms["windows-x86_64"],
    manifest.platforms["windows-x86_64-nsis"],
  );
});

test("an installer without a signature breaks the release", () => {
  const root = bundle();
  rmSync(join(root, "nsis", "MinLauncher_0.1.0_x64-setup.exe.sig"));
  assert.throws(() => collectArtifacts(root), /signature/i);
});
