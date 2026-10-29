#!/usr/bin/env node

/** Publish the current local changes as a signed GitHub release. */

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createInterface } from "node:readline/promises";
import { stdin, stdout } from "node:process";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";

import { isSensitivePath, nextPatchVersion } from "./publish-release-utils.mjs";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const REPOSITORY = "akley-design/MinLauncher";
const REPOSITORY_URL = `https://github.com/${REPOSITORY}.git`;
const REQUIRED_SECRETS = [
  "TAURI_SIGNING_PRIVATE_KEY",
  "TAURI_SIGNING_PRIVATE_KEY_PASSWORD",
  "WINDOWS_CERTIFICATE_PFX_BASE64",
  "WINDOWS_CERTIFICATE_PASSWORD",
];
const REQUIRED_VARIABLES = ["VITE_SUPABASE_URL", "VITE_SUPABASE_ANON_KEY"];

function run(file, args, options = {}) {
  const result = spawnSync(file, args, {
    cwd: options.cwd ?? ROOT,
    encoding: "utf8",
    windowsHide: true,
    maxBuffer: 16 * 1024 * 1024,
  });
  if (result.error)
    throw new Error(`${file} could not start: ${result.error.message}`);
  if (!options.quiet && result.stdout) stdout.write(result.stdout);
  if (!options.quiet && result.stderr) stderrWrite(result.stderr);
  if (result.status !== 0 && !options.allowFailure) {
    throw new Error(
      `${file} ${args.join(" ")} failed${result.stderr ? `:\n${result.stderr.trim()}` : "."}`,
    );
  }
  return {
    code: result.status ?? 1,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? "",
  };
}

function stderrWrite(message) {
  process.stderr.write(message);
}

function output(file, args, options = {}) {
  return run(file, args, { ...options, quiet: true }).stdout.trim();
}

function readJson(path) {
  return JSON.parse(readFileSync(join(ROOT, path), "utf8"));
}

function writeJson(path, value) {
  const absolutePath = join(ROOT, path);
  const original = readFileSync(absolutePath, "utf8");
  const eol = original.includes("\r\n") ? "\r\n" : "\n";
  writeFileSync(
    absolutePath,
    `${JSON.stringify(value, null, 2).replaceAll("\n", eol)}${eol}`,
  );
}

function packageMetadata() {
  const metadata = JSON.parse(
    output("cargo", [
      "metadata",
      "--no-deps",
      "--format-version",
      "1",
      "--quiet",
    ]),
  );
  return metadata.packages;
}

function workspaceVersion() {
  const cargo = readFileSync(join(ROOT, "Cargo.toml"), "utf8");
  const match =
    /^\[workspace\.package\]\s*\r?\n(?:(?!^\[).)*?^version\s*=\s*"([^"]+)"/ms.exec(
      cargo,
    );
  if (!match)
    throw new Error(
      "Could not read [workspace.package].version from Cargo.toml.",
    );
  return match[1];
}

function localVersion() {
  const rust = workspaceVersion();
  const tauri = readJson("crates/ml-desktop/tauri.conf.json").version;
  const npm = readJson("package.json").version;
  if (rust !== tauri || rust !== npm) {
    throw new Error(
      `Project versions differ: Cargo=${rust}, Tauri=${tauri}, package.json=${npm}. Fix them before releasing.`,
    );
  }
  return rust;
}

function dirtyPaths() {
  const paths = new Set([
    ...output("git", [
      "ls-files",
      "--modified",
      "--deleted",
      "--others",
      "--exclude-standard",
      "-z",
    ]).split("\0"),
    ...output("git", ["diff", "--cached", "--name-only", "-z"]).split("\0"),
  ]);
  paths.delete("");
  return [...paths];
}

function updateVersion(nextVersion, metadata) {
  const current = localVersion();
  const cargoPath = join(ROOT, "Cargo.toml");
  let cargo = readFileSync(cargoPath, "utf8");
  const workspaceSection =
    /(^\[workspace\.package\]\s*\r?\n)([\s\S]*?)(?=^\[|\s*$)/m;
  const section = cargo.match(workspaceSection);
  if (!section)
    throw new Error("Could not locate [workspace.package] in Cargo.toml.");
  const updatedSection = section[0].replace(
    new RegExp(`(^version\\s*=\\s*)"${current.replaceAll(".", "\\.")}"`, "m"),
    `$1"${nextVersion}"`,
  );
  if (updatedSection === section[0])
    throw new Error("Could not update the Cargo workspace version.");
  cargo = cargo.replace(section[0], updatedSection);
  // Workspace path dependencies repeat the workspace version in Cargo.toml.
  cargo = cargo.replaceAll(
    `version = "${current}"`,
    `version = "${nextVersion}"`,
  );
  writeFileSync(cargoPath, cargo);

  const tauriPath = join(ROOT, "crates/ml-desktop/tauri.conf.json");
  const tauri = JSON.parse(readFileSync(tauriPath, "utf8"));
  tauri.version = nextVersion;
  writeJson("crates/ml-desktop/tauri.conf.json", tauri);

  const packageFiles = [
    "package.json",
    "apps/ui/package.json",
    "apps/site/package.json",
  ];
  const npmPackages = new Map();
  for (const path of packageFiles) {
    const pkg = readJson(path);
    npmPackages.set(
      path === "package.json" ? "" : path.replace("/package.json", ""),
      pkg.version,
    );
    pkg.version = nextVersion;
    writeJson(path, pkg);
  }

  const lock = readJson("package-lock.json");
  lock.version = nextVersion;
  if (lock.packages?.[""]) lock.packages[""].version = nextVersion;
  for (const [path] of npmPackages) {
    if (path && lock.packages?.[path])
      lock.packages[path].version = nextVersion;
  }
  writeJson("package-lock.json", lock);

  const crateNames = new Set(metadata.map((pkg) => pkg.name));
  const lockPath = join(ROOT, "Cargo.lock");
  const lockText = readFileSync(lockPath, "utf8");
  let changedPackages = 0;
  const updatedLock = lockText
    .split(/(?=^\[\[package\]\]\s*$)/m)
    .map((block) => {
      const name = /^name\s*=\s*"([^"]+)"/m.exec(block)?.[1];
      const version = /^version\s*=\s*"([^"]+)"/m.exec(block)?.[1];
      if (!crateNames.has(name) || version !== current) return block;
      changedPackages += 1;
      return block.replace(
        /^version\s*=\s*"[^"]+"/m,
        `version = "${nextVersion}"`,
      );
    })
    .join("");
  if (changedPackages !== crateNames.size) {
    throw new Error(
      `Cargo.lock updated ${changedPackages} of ${crateNames.size} workspace package versions; stopping to avoid an inconsistent release.`,
    );
  }
  writeFileSync(lockPath, updatedLock);
}

function parseTags(text) {
  return text
    .split(/\r?\n/)
    .filter(Boolean)
    .map((tag) => {
      const match = /^v(\d+)\.(\d+)\.(\d+)$/.exec(tag);
      return match ? { tag, parts: match.slice(1).map(Number) } : null;
    })
    .filter(Boolean)
    .sort((a, b) => {
      for (let i = 0; i < 3; i += 1)
        if (a.parts[i] !== b.parts[i]) return b.parts[i] - a.parts[i];
      return 0;
    });
}

function releasePlan(current, latestTag) {
  if (!latestTag) return { version: nextPatchVersion(current), resume: false };
  const releasedVersion = latestTag.replace(/^v/, "");
  if (current === releasedVersion)
    return { version: nextPatchVersion(current), resume: false };
  if (current === nextPatchVersion(releasedVersion))
    return { version: current, resume: true };
  throw new Error(
    `Local version ${current} does not match latest release ${latestTag}. Reconcile versions before publishing.`,
  );
}

async function ask(question) {
  const terminal = createInterface({ input: stdin, output: stdout });
  try {
    return (await terminal.question(question)).trim();
  } finally {
    terminal.close();
  }
}

function ensureGitHubConfiguration() {
  const secretNames = output(
    "gh",
    [
      "secret",
      "list",
      "--repo",
      REPOSITORY,
      "--json",
      "name",
      "--jq",
      ".[].name",
    ],
    { allowFailure: true },
  )
    .split(/\r?\n/)
    .filter(Boolean);
  const variableNames = output(
    "gh",
    [
      "variable",
      "list",
      "--repo",
      REPOSITORY,
      "--json",
      "name",
      "--jq",
      ".[].name",
    ],
    { allowFailure: true },
  )
    .split(/\r?\n/)
    .filter(Boolean);
  const missingSecrets = REQUIRED_SECRETS.filter(
    (name) => !secretNames.includes(name),
  );
  const missingVariables = REQUIRED_VARIABLES.filter(
    (name) => !variableNames.includes(name),
  );
  if (missingSecrets.length || missingVariables.length) {
    const lines = ["GitHub Actions is missing release configuration:"];
    for (const name of missingSecrets)
      lines.push(`  gh secret set ${name} --repo ${REPOSITORY}`);
    for (const name of missingVariables)
      lines.push(`  gh variable set ${name} --repo ${REPOSITORY}`);
    throw new Error(lines.join("\n"));
  }
}

function ensureNoSensitiveFiles(paths) {
  const sensitive = paths.filter(isSensitivePath);
  if (sensitive.length) {
    throw new Error(
      `Refusing to publish likely secrets:\n${sensitive.map((path) => `  ${path}`).join("\n")}`,
    );
  }
}

function printReleaseRequirements() {
  process.stderr.write(
    `\nFirst configure GitHub Actions for ${REPOSITORY}:\n` +
      "  Secrets: TAURI_SIGNING_PRIVATE_KEY, TAURI_SIGNING_PRIVATE_KEY_PASSWORD, " +
      "WINDOWS_CERTIFICATE_PFX_BASE64, WINDOWS_CERTIFICATE_PASSWORD\n" +
      "  Variables: VITE_SUPABASE_URL, VITE_SUPABASE_ANON_KEY\n" +
      "The private signing key must match the public key in tauri.conf.json.\n",
  );
}

async function main() {
  if (!existsSync(join(ROOT, ".git")))
    throw new Error(`Not a Git repository: ${ROOT}`);
  run("gh", ["auth", "status", "--hostname", "github.com"], { quiet: true });
  const remote = output("git", ["remote", "get-url", "origin"], {
    allowFailure: true,
  });
  if (
    remote &&
    ![
      REPOSITORY_URL,
      `https://github.com/${REPOSITORY}.git/`,
      `git@github.com:${REPOSITORY}.git`,
    ].includes(remote)
  ) {
    throw new Error(
      `origin points to ${remote}; this publisher only targets ${REPOSITORY_URL}.`,
    );
  }

  const status = output("git", ["status", "--short", "--untracked-files=all"]);
  const paths = dirtyPaths();
  ensureNoSensitiveFiles(paths);
  if (status) {
    process.stdout.write("\nLocal changes that will be included:\n");
    process.stdout.write(`${status}\n`);
    const diff = output("git", ["diff", "--stat", "HEAD"]);
    if (diff) process.stdout.write(`\n${diff}\n`);
  }

  const repoStatus = run(
    "gh",
    ["repo", "view", REPOSITORY, "--json", "url", "--jq", ".url"],
    { quiet: true, allowFailure: true },
  );
  const repoExists = repoStatus.code === 0;
  const tags = repoExists
    ? parseTags(
        output("gh", [
          "release",
          "list",
          "--repo",
          REPOSITORY,
          "--limit",
          "100",
          "--json",
          "tagName,isDraft,isPrerelease",
          "--jq",
          ".[] | select(.isDraft == false and .isPrerelease == false) | .tagName",
        ]),
      )
    : [];
  const current = localVersion();
  const latestTag = tags[0]?.tag ?? "";
  const plan = releasePlan(current, latestTag);
  const versionTag = `v${plan.version}`;
  const releaseNotesPath = join(ROOT, "release-notes", `${versionTag}.md`);
  const hasSavedReleaseNotes = existsSync(releaseNotesPath);
  const branch = output("git", ["branch", "--show-current"]);
  if (!branch)
    throw new Error(
      "The project is in detached HEAD state; switch to a branch and retry.",
    );

  if (latestTag && !status) {
    const localTag = run(
      "git",
      ["rev-parse", "--verify", "--quiet", `${latestTag}^{}`],
      { quiet: true, allowFailure: true },
    );
    if (
      localTag.code === 0 &&
      localTag.stdout.trim() === output("git", ["rev-parse", "HEAD"])
    ) {
      process.stdout.write(
        `No local changes since ${latestTag}; no release was created.\n`,
      );
      return;
    }
  }

  process.stdout.write(`\nRepository: https://github.com/${REPOSITORY}\n`);
  process.stdout.write(`Latest published release: ${latestTag || "none"}\n`);
  process.stdout.write(
    `Release to publish: ${versionTag}${plan.resume ? " (resume pending release)" : ""}\n`,
  );
  if (!repoExists)
    process.stdout.write(
      "The public GitHub repository does not exist yet; it will be created after confirmation.\n",
    );
  const releaseNotes =
    plan.resume || hasSavedReleaseNotes
      ? readFileSync(releaseNotesPath, "utf8").trim()
      : await ask("Описание обновления для пользователей: ");
  if (!releaseNotes) throw new Error("Release notes cannot be empty.");
  process.stdout.write(`\nUpdate description:\n${releaseNotes}\n`);
  process.stdout.write(
    "The publisher will stage all non-ignored project changes, update versions, commit, push, and tag the release.\n",
  );
  const confirmation = await ask(
    "Publish this project publicly and start the release? Type YES to continue: ",
  );
  if (confirmation !== "YES") {
    process.stdout.write("Cancelled. No project changes were published.\n");
    return;
  }

  if (!repoExists) run("gh", ["repo", "create", REPOSITORY, "--public"]);
  if (!remote) run("git", ["remote", "add", "origin", REPOSITORY_URL]);

  if (latestTag) {
    run("git", ["fetch", "--quiet", "origin", "tag", latestTag]);
    if (!status && !plan.resume) {
      const ahead = Number(
        output("git", ["rev-list", "--count", `${latestTag}..HEAD`]),
      );
      if (ahead === 0) {
        process.stdout.write(
          `No local changes since ${latestTag}; no release was created.\n`,
        );
        return;
      }
    }
  }
  ensureGitHubConfiguration();

  const metadata = packageMetadata();
  if (!plan.resume) updateVersion(plan.version, metadata);
  if (!hasSavedReleaseNotes) {
    mkdirSync(join(ROOT, "release-notes"), { recursive: true });
    writeFileSync(releaseNotesPath, `${releaseNotes}\n`);
  }

  run("git", ["add", "-A"]);
  const stagedPaths = output("git", ["diff", "--cached", "--name-only", "-z"])
    .split("\0")
    .filter(Boolean);
  ensureNoSensitiveFiles(stagedPaths);
  const staged = run("git", ["diff", "--cached", "--quiet"], {
    quiet: true,
    allowFailure: true,
  });
  if (staged.code !== 0) {
    run("git", ["commit", "-m", `Release MinLauncher ${versionTag}`]);
  } else if (!plan.resume) {
    throw new Error("There are no changes to commit or release.");
  }
  run("git", ["push", "--set-upstream", "origin", `HEAD:refs/heads/${branch}`]);

  const existingTag = run(
    "git",
    ["rev-parse", "--verify", "--quiet", `refs/tags/${versionTag}`],
    { quiet: true, allowFailure: true },
  );
  if (existingTag.code !== 0) {
    run("git", ["tag", "-a", versionTag, "-m", `MinLauncher ${versionTag}`]);
  } else {
    const taggedCommit = output("git", ["rev-parse", `${versionTag}^{}`]);
    const headCommit = output("git", ["rev-parse", "HEAD"]);
    if (taggedCommit !== headCommit)
      throw new Error(`${versionTag} already points to a different commit.`);
  }
  run("git", ["push", "origin", `refs/tags/${versionTag}`]);
  process.stdout.write(
    `\nRelease ${versionTag} was pushed. GitHub Actions is building and publishing the signed installers.\n`,
  );
  process.stdout.write(
    `Track the release at https://github.com/${REPOSITORY}/actions\n`,
  );
}

main().catch((error) => {
  process.stderr.write(`\nRelease stopped: ${error.message}\n`);
  if (error.message.includes("GitHub Actions is missing release configuration"))
    printReleaseRequirements();
  process.exitCode = 1;
});
