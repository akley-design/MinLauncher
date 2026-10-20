#!/usr/bin/env node
// The manifest of the update.
//
// The installed launcher asks the endpoint for this file and learns from it
// two things: which file to download, and which signature proves that the
// downloaded file is the published one. The signature is the content of the
// `.sig` file that the bundler writes next to every installer — the updater
// decodes it from base64 and checks it against the public key from
// `tauri.conf.json`.
//
// The keys of `platforms` are looked up in this order: `{os}-{arch}-{kind}`
// and then `{os}-{arch}`. The first one picks the installer of the same kind
// as the running one (NSIS stays NSIS, MSI stays MSI), the second one is the
// fallback for the clients that know nothing about installer kinds.
//
// Usage:
//   node scripts/latest-json.mjs --bundle-dir <dir> --version <version> \
//     --base-url <url> [--notes <text>] [--out <file>]

import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";

const TARGET = "windows-x86_64";

/**
 * The newest file of the directory with the given extension.
 *
 * shortcut: the newest by the time of writing, not by the name — the bundler
 * writes one installer per kind, so a second file is only a leftover of an
 * earlier build. Upgrade to an exact name when the bundler ever writes two
 * installers of the same kind.
 */
function newestFile(dir, extension) {
  if (!existsSync(dir)) return null;
  const files = readdirSync(dir).filter((name) => name.endsWith(extension));
  if (files.length === 0) return null;
  const [newest] = files
    .map((name) => ({ name, time: statSync(join(dir, name)).mtimeMs }))
    .sort((a, b) => b.time - a.time)
    .map((entry) => entry.name);
  return join(dir, newest);
}

/**
 * The installers of the bundle with the signature of each of them.
 *
 * A missing signature is an error and not a warning: the updater refuses to
 * install an artifact it cannot check, and that must break the release
 * instead of reaching a player.
 */
export function collectArtifacts(bundleDir) {
  const artifacts = {};
  for (const [kind, directory, extension] of [
    ["nsis", "nsis", ".exe"],
    ["msi", "msi", ".msi"],
  ]) {
    const file = newestFile(join(bundleDir, directory), extension);
    if (!file) continue;
    const signaturePath = `${file}.sig`;
    if (!existsSync(signaturePath)) {
      throw new Error(
        `The signature of ${basename(file)} is missing (${signaturePath}). ` +
          "The release cannot be published without it.",
      );
    }
    artifacts[kind] = {
      name: basename(file),
      signature: readFileSync(signaturePath, "utf8").trim(),
    };
  }
  if (Object.keys(artifacts).length === 0) {
    throw new Error(`No installer was found in ${bundleDir}.`);
  }
  return artifacts;
}

/**
 * The manifest itself.
 */
export function buildManifest({ version, notes, pubDate, baseUrl, artifacts }) {
  const root = baseUrl.replace(/\/+$/, "");
  const platforms = {};
  for (const [kind, artifact] of Object.entries(artifacts)) {
    platforms[`${TARGET}-${kind}`] = {
      signature: artifact.signature,
      url: `${root}/${artifact.name}`,
    };
  }
  // The fallback for the clients that do not know about installer kinds: they
  // ask for `{os}-{arch}` and get the NSIS installer.
  if (artifacts.nsis) {
    platforms[TARGET] = {
      signature: artifacts.nsis.signature,
      url: `${root}/${artifacts.nsis.name}`,
    };
  }
  return { version, notes, pub_date: pubDate, platforms };
}

/**
 * The arguments of the command line, as a plain object.
 */
function parseArgs(argv) {
  const args = {};
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index];
    if (!key?.startsWith("--")) {
      throw new Error(`Unexpected argument: ${key}`);
    }
    args[key.slice(2)] = argv[index + 1];
  }
  for (const required of ["bundle-dir", "version", "base-url"]) {
    if (!args[required]) {
      throw new Error(`The argument --${required} is required.`);
    }
  }
  return args;
}

function main(argv) {
  const args = parseArgs(argv);
  const manifest = buildManifest({
    version: args.version,
    notes: args.notes ?? "",
    pubDate: args["pub-date"] ?? new Date().toISOString(),
    baseUrl: args["base-url"],
    artifacts: collectArtifacts(args["bundle-dir"]),
  });
  const json = `${JSON.stringify(manifest, null, 2)}\n`;
  if (args.out) {
    writeFileSync(args.out, json);
    console.log(`Wrote ${args.out}`);
  } else {
    process.stdout.write(json);
  }
}

if (process.argv[1] && import.meta.url.endsWith(basename(process.argv[1]))) {
  main(process.argv.slice(2));
}
