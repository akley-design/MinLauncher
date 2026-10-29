/**
 * The typed client of the backend.
 *
 * Every function here is one Tauri command. The name of the function is
 * the name of the command, and the arguments are the arguments of the
 * command in the form the frontend has them.
 *
 * Tauri 2 converts the arguments of a command from camelCase to
 * snake_case on the Rust side, so `versionId` reaches a command that
 * declares `version_id`. The names below follow that rule.
 *
 * The commands fail with an [`IpcError`], which the backend builds
 * from the error of the domain. The frontend switches on the `code` and
 * shows the `message`.
 */

import { invoke } from "@tauri-apps/api/core";

import type {
  AccountList,
  BenchmarkList,
  DownloadEntry,
  InstallOptions,
  InstallResult,
  InGameComparisons,
  IpcError,
  InstanceList,
  LaunchOptions,
  LaunchResult,
  ManifestSummary,
  ModloaderCatalog,
  PerformanceModInstall,
  RuntimeResult,
  RuntimeView,
  SettingsView,
  TLauncherBaselineImport,
  UpdateView,
  VersionList,
} from "./types";

/**
 * The failure of a command, in the form the frontend can use.
 *
 * A command that fails for a reason the backend did not expect — a
 * broken IPC bridge, a webview that was closed mid-call — does not
 * produce an `IpcError`. It produces whatever the runtime throws. The
 * two are merged here, so that a caller only ever has to handle one
 * shape of failure.
 */
export class CommandError extends Error {
  /** The machine-readable kind of the failure. */
  readonly code: IpcError["code"];

  /** The underlying cause, when there is one worth showing. */
  readonly detail: string | undefined;

  constructor(code: IpcError["code"], message: string, detail?: string) {
    super(message);
    this.name = "CommandError";
    this.code = code;
    this.detail = detail;
  }
}

/**
 * Turn whatever a command threw into a [`CommandError`].
 *
 * The backend reports its failures as an object with a `code`. Anything
 * else is a failure of the bridge itself, which is an internal one.
 */
export function toCommandError(error: unknown): CommandError {
  if (error instanceof CommandError) {
    return error;
  }

  if (typeof error === "object" && error !== null && "code" in error && "message" in error) {
    const candidate = error as IpcError;
    return new CommandError(candidate.code, candidate.message, candidate.detail);
  }

  if (error instanceof Error) {
    return new CommandError("internal", error.message);
  }

  return new CommandError("internal", String(error));
}

/**
 * Call a command and turn its failure into a [`CommandError`].
 *
 * Every command goes through here, so that no caller ever sees the raw
 * shape the runtime throws.
 */
async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw toCommandError(error);
  }
}

// --- the versions ----------------------------------------------------

/** The list of the versions the launcher knows about. */
export function listVersions(): Promise<VersionList> {
  return call<VersionList>("list_versions");
}

/** Fetch the manifest from Mojang and remember what it says. */
export function refreshManifest(): Promise<ManifestSummary> {
  return call<ManifestSummary>("refresh_manifest");
}

/** Install a version of the game. */
export function installVersion(options: InstallOptions): Promise<InstallResult> {
  return call<InstallResult>("install_version", { options });
}

// --- the runtimes ----------------------------------------------------

/** The Java runtimes the launcher can download for this machine. */
export function listRuntimes(): Promise<RuntimeView[]> {
  return call<RuntimeView[]>("list_runtimes");
}

/** Download a Java runtime and unpack it. */
export function installRuntime(component: string): Promise<RuntimeResult> {
  return call<RuntimeResult>("install_runtime", { component });
}

// --- the download queue -----------------------------------------------

/** The entries of the download queue. */
export function listDownloads(): Promise<DownloadEntry[]> {
  return call<DownloadEntry[]>("list_downloads");
}

/** Remove the finished entries from the queue. */
export function clearCompletedDownloads(): Promise<number> {
  return call<number>("clear_completed_downloads");
}

/** Remove the failed entries from the queue. */
export function clearFailedDownloads(): Promise<number> {
  return call<number>("clear_failed_downloads");
}

/** Fetch one file of the queue again. */
export function retryDownload(url: string): Promise<DownloadEntry> {
  return call<DownloadEntry>("retry_download", { url });
}

// --- the accounts ----------------------------------------------------

/** The accounts of the launcher, and which one is selected. */
export function listAccounts(): Promise<AccountList> {
  return call<AccountList>("list_accounts");
}

/** Select the account the game is started with. */
export function selectAccount(accountId: string): Promise<AccountList> {
  return call<AccountList>("select_account", { accountId });
}

/** Get the Minecraft offline identity for a nickname before reserving it online. */
export function offlineAccountIdentity(
  username: string,
): Promise<import("./types").OfflineIdentity> {
  return call<import("./types").OfflineIdentity>("offline_account_identity", { username });
}

/** Add an offline account after the nickname registry granted the claim. */
export function addOfflineAccount(username: string, claimToken: string): Promise<AccountList> {
  return call<AccountList>("add_offline_account", { username, claimToken });
}

/** Sync a registered MinLauncher profile to the local game account list. */
export function upsertMinLauncherAccount(
  nickname: string,
  playerUuid: string,
): Promise<AccountList> {
  return call<AccountList>("upsert_minlauncher_account", { nickname, playerUuid });
}

/** Redeem and locally store a short-lived Minecraft account handoff. */
export function redeemMinecraftHandoff(code: string): Promise<AccountList> {
  const url = import.meta.env.VITE_SUPABASE_URL?.trim();
  const anonKey = import.meta.env.VITE_SUPABASE_ANON_KEY?.trim();
  if (!url || !anonKey)
    return Promise.reject(new Error("Вход Microsoft не настроен в этой сборке."));
  return call<AccountList>("redeem_minecraft_handoff", {
    endpoint: `${url.replace(/\/$/, "")}/functions/v1/redeem-minecraft-handoff`,
    anonKey,
    code,
  });
}

/** Restore a non-secret game account from encrypted cloud sync data. */
export function upsertSyncedAccount(account: import("./types").Account): Promise<AccountList> {
  return call<AccountList>("upsert_synced_account", { account });
}

/** Encrypt account metadata and credentials locally; only ciphertext leaves the app. */
export function encryptAccountSyncPayload(accountId: string, password: string): Promise<number[]> {
  return call<number[]>("encrypt_account_sync_payload", { accountId, password });
}

/** Decrypt a cloud account and restore credentials inside Rust without exposing tokens to UI code. */
export function restoreAccountSyncPayload(
  accountId: string,
  payload: number[],
  password: string,
): Promise<import("./types").Account> {
  return call<import("./types").Account>("restore_account_sync_payload", {
    accountId,
    payload,
    password,
  });
}

export function setNicknameClaimStatus(
  accountId: string,
  nickname: string,
  status: "reserved" | "conflict",
): Promise<void> {
  return call<void>("set_nickname_claim_status", { accountId, nickname, status });
}

export function clearNicknameClaim(accountId: string): Promise<void> {
  return call<void>("clear_nickname_claim", { accountId });
}

export function queueNicknameRelease(accountId: string, nickname: string): Promise<void> {
  return call<void>("queue_nickname_release", { accountId, nickname });
}

/** Remove an account. */
export function removeAccount(accountId: string): Promise<AccountList> {
  return call<AccountList>("remove_account", { accountId });
}

// --- the instances ---------------------------------------------------

/** The instances of the launcher. */
export function listInstances(): Promise<InstanceList> {
  return call<InstanceList>("list_instances");
}

/** Create an instance. */
export function createInstance(name: string, versionId: string): Promise<InstanceList> {
  return call<InstanceList>("create_instance", { name, versionId });
}

/** Stable Forge/Fabric choices and the newest stable Fabric API per game release. */
export function listModloaderVersions(): Promise<ModloaderCatalog> {
  return call<ModloaderCatalog>("list_modloader_versions");
}

/** Install the chosen modloader profile and create its isolated instance. */
export function createModloaderInstance(options: {
  name: string;
  minecraft_version: string;
  loader: "forge" | "fabric";
}): Promise<InstanceList> {
  return call<InstanceList>("create_modloader_instance", { options });
}

/** Copy the local Forge profile into a private MinLauncher instance. */
export function importForgeInstance(sourceProfileId: string, name: string): Promise<InstanceList> {
  return call<InstanceList>("import_forge_instance", {
    sourceProfileId,
    name,
    sourceGameDir: null,
  });
}

/** Install the checksum-pinned Forge 1.20.1 performance bundle into a private instance. */
export function installPerformanceMods(instanceId: string): Promise<PerformanceModInstall> {
  return call<PerformanceModInstall>("install_performance_mods", { instanceId });
}

/** Remove an instance. Its directory is kept. */
export function removeInstance(instanceId: string): Promise<InstanceList> {
  return call<InstanceList>("remove_instance", { instanceId });
}

// --- the settings ----------------------------------------------------

/** The settings as they are now. */
export function readSettings(): Promise<SettingsView> {
  return call<SettingsView>("read_settings");
}

/** Write the settings. */
export function writeSettings(config: import("./types").LauncherConfig): Promise<SettingsView> {
  return call<SettingsView>("write_settings", { config });
}

/** Ask for the directory that holds the game files. */
export function pickInstallDirectory(): Promise<string | null> {
  return call<string | null>("pick_install_directory");
}

// --- the updates of the launcher itself -------------------------------

/**
 * Ask whether there is a newer version of the launcher.
 *
 * A launcher that has no server of the updates to ask answers
 * `configured: false`: that is a state, not a failure.
 */
export function checkForUpdate(): Promise<UpdateView> {
  return call<UpdateView>("check_for_update");
}

/**
 * Download the announced update and put it in place.
 *
 * The manifest of the release comes from the check, so that the version
 * that is installed is the one that was shown.
 */
export function installUpdate(rawJson: unknown): Promise<void> {
  return call<void>("install_update", { rawJson });
}

// --- the measurements ------------------------------------------------

/** The newest measurement of every metric. */
export function latestBenchmarks(): Promise<BenchmarkList> {
  return call<BenchmarkList>("latest_benchmarks");
}

/** The comparison of the game with the mod and without it. */
export function compareInGame(): Promise<InGameComparisons> {
  return call<InGameComparisons>("compare_in_game");
}

/** Import a TLauncher baseline produced by the local FPS collector. */
export function importTLauncherBaseline(samplesJson: string): Promise<TLauncherBaselineImport> {
  return call<TLauncherBaselineImport>("import_tlauncher_baseline", { samplesJson });
}

// --- the game --------------------------------------------------------

/** Start the game. */
export function launchGame(options: LaunchOptions): Promise<LaunchResult> {
  return call<LaunchResult>("launch_game", { options });
}

/** Stop the game. */
export function stopGame(): Promise<void> {
  return call<void>("stop_game");
}
