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
  Directories,
  DownloadEntry,
  InstallOptions,
  InstallResult,
  IpcError,
  InstanceList,
  LaunchOptions,
  LaunchResult,
  ManifestSummary,
  RuntimeResult,
  RuntimeView,
  SettingsView,
  VersionList,
  VersionManifest,
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

/** The manifest as the launcher last saw it. */
export function readManifest(): Promise<VersionManifest | null> {
  return call<VersionManifest | null>("read_manifest");
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

/** The account that is selected. */
export function selectedAccount(): Promise<import("./types").Account | null> {
  return call<import("./types").Account | null>("selected_account");
}

/** Select the account the game is started with. */
export function selectAccount(accountId: string): Promise<AccountList> {
  return call<AccountList>("select_account", { accountId });
}

/** Add an offline account. */
export function addOfflineAccount(username: string): Promise<AccountList> {
  return call<AccountList>("add_offline_account", { username });
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

/** The directories the launcher uses. */
export function readDirectories(): Promise<Directories> {
  return call<Directories>("read_directories");
}

// --- the measurements ------------------------------------------------

/** The newest measurement of every metric. */
export function latestBenchmarks(): Promise<BenchmarkList> {
  return call<BenchmarkList>("latest_benchmarks");
}

/** The stored measurements of a metric and a scenario. */
export function listBenchmarks(metric: string, scenario: string): Promise<BenchmarkList> {
  return call<BenchmarkList>("list_benchmarks", { metric, scenario });
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
