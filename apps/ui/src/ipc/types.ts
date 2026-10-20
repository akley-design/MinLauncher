/**
 * The types that cross the IPC boundary.
 *
 * Every one of them mirrors a Rust type of the `ml-desktop` crate, and
 * the names of the fields are the names of the JSON the Rust side
 * produces. When a Rust type changes, the matching type here changes
 * with it — the compiler is what keeps the two halves together, because
 * `invoke` is typed by the generic that is given to it.
 *
 * The Rust side is the truth. This file is a copy of it, and the copy is
 * checked by the type checker on every build.
 */

/** The kind of a version, as Mojang classifies it. */
export type VersionKind = "release" | "snapshot" | "old_beta" | "old_alpha";

/** The kind of an account. */
export type AccountType = "microsoft" | "mojang" | "offline" | "third_party";

/** The machine-readable kind of a failure. */
export type ErrorCode =
  "invalid_input" | "not_found" | "storage" | "network" | "launch" | "unimplemented" | "internal";

/** A failure that a command reported. */
export interface IpcError {
  code: ErrorCode;
  message: string;
  detail?: string;
}

/** A version, as the frontend sees it. */
export interface VersionView {
  id: string;
  kind: VersionKind;
  release_time?: string;
  installed: boolean;
  java_major?: number;
  asset_index_id?: string;
}

/** The list of the versions and the latest of them. */
export interface VersionList {
  versions: VersionView[];
  latest_release: string;
  latest_snapshot: string;
  installed_count: number;
}

/** What the launcher learned from the manifest. */
export interface ManifestSummary {
  latest_release: string;
  latest_snapshot: string;
  total: number;
}

/** A player account. Never carries a token. */
export interface Account {
  id: string;
  uuid: string;
  username: string;
  kind: AccountType;
  uuid_prefix: string;
  selected: boolean;
  created_at: string;
}

/** The accounts of the launcher, and which one is selected. */
export interface AccountList {
  accounts: Account[];
  selected_id: string | null;
}

/** A game instance. */
export interface Instance {
  id: string;
  version_id: string;
  name: string;
  game_dir: string;
  java_path: string | null;
  jvm_args: string | null;
  created_at: string;
}

/** The instances of the launcher. */
export interface InstanceList {
  instances: Instance[];
}

/** Download engine tuning. */
export interface DownloadConfig {
  max_concurrent: number;
  max_retries: number;
  timeout_secs: number;
  verify_checksums: boolean;
  bandwidth_limit_kbps: number;
}

/** JVM tuning exposed to the user. */
export interface JvmConfig {
  max_heap: string;
  min_heap: string;
  garbage_collector: string;
  extra_args: string[];
  enable_agent: boolean;
}

/** The settings of the mods. */
export interface ModConfig {
  enabled: boolean;
  loader_version: string;
  mod_jar: string;
  preset: string;
}

/** The rules of the restart of a game that fell over. */
export interface RestartConfig {
  enabled: boolean;
  max_restarts: number;
  backoff_ms: number;
  window_ms: number;
}

/**
 * The launcher's complete settings.
 *
 * The copy of the Rust type has to be whole: the settings are read
 * whole and written whole, and a field that is missing here is a field
 * that goes back to its default when the player presses «Сохранить».
 */
export interface LauncherConfig {
  schema_version: number;
  language: string;
  minimize_to_tray: boolean;
  autostart: boolean;
  default_version: string | null;
  default_jre: string;
  show_console: boolean;
  downloads: DownloadConfig;
  jvm: JvmConfig;
  mods: ModConfig;
  restart: RestartConfig;
  telemetry_enabled: boolean;
}

/** The directories the launcher uses. */
export interface Directories {
  data: string;
  cache: string;
  database: string;
}

/** The settings as they are now, together with the directories. */
export interface SettingsView {
  config: LauncherConfig;
  directories: Directories;
  /** Whether this system can start the launcher with itself. */
  autostart_supported: boolean;
}

/** The state of one entry of the download queue. */
export type DownloadState = "queued" | "downloading" | "completed" | "failed";

/**
 * One entry of the download queue.
 *
 * The queue is what the launcher remembers about the files it fetched.
 * The key of an entry is its address, so two versions that share a
 * library share one entry.
 */
export interface DownloadEntry {
  /** The address the payload comes from. */
  url: string;
  /** The expected SHA-1 of the payload, when it is known. */
  sha1: string | null;
  /** The expected size of the payload, when it is known. */
  size: number | null;
  /** Where the payload is written. */
  target_path: string;
  /** Where the entry stands now. */
  state: DownloadState;
  /** How many bytes of the payload are on the disk. */
  bytes_done: number;
  /** How many attempts have been made. */
  attempts: number;
  /** The reason of the last failure, when there was one. */
  error: string | null;
  /** When the entry was last touched, ISO-8601. */
  updated_at: string;
}

/** One stored measurement. */
export interface BenchmarkView {
  suite: string;
  metric: string;
  scenario: string;
  samples: number;
  p50: number;
  p95: number;
  p99: number;
  mean: number;
  stddev: number;
  unit: string;
  created_at: string;
}

/** The stored measurements. */
export interface BenchmarkList {
  records: BenchmarkView[];
}

/** The state of the running game. */
export interface RunningGame {
  pid: number | null;
  version_id: string;
  command: string;
  started_at: number;
}

/** What the frontend gets after a successful start. */
export interface LaunchResult {
  game: RunningGame;
  prepare_ms: number;
}

/** The request of a launch. */
export interface LaunchOptions {
  version_id: string;
  instance_id?: string;
  width?: number;
  height?: number;
  java_path?: string;
}

/**
 * What the launcher tells the frontend while the game runs.
 *
 * The three kinds are the three moments of a run of the game: it
 * started, it fell over and is being started again, it ended.
 */
export type GameEvent =
  | { kind: "started"; game: RunningGame }
  | { kind: "restarting"; version_id: string; attempt: number }
  | {
      kind: "exited";
      version_id: string;
      success: boolean;
      code: number | null;
      /** Whether the player stopped the game on purpose. */
      stopped: boolean;
    };

/** The request of an installation. */
export interface InstallOptions {
  version_id: string;
  allow_network?: boolean;
}

/** What the frontend gets after a successful installation. */
export interface InstallResult {
  version_id: string;
  files: number;
  bytes: number;
  elapsed_ms: number;
}

/**
 * What the launcher tells the frontend while it installs a version.
 *
 * The four kinds are the four moments of an installation: it begins, it
 * makes progress, it ends, or it fails.
 */
export type InstallEvent =
  | { kind: "started"; version_id: string; total: number; bytes: number }
  | {
      kind: "progress";
      version_id: string;
      completed: number;
      total: number;
      bytes: number;
    }
  | { kind: "finished"; version_id: string; files: number; bytes: number; elapsed_ms: number }
  | { kind: "failed"; version_id: string; message: string };

/**
 * A Java runtime the launcher can download.
 *
 * The runtimes are published by Mojang for every platform; the launcher
 * only shows the ones that fit the machine it runs on.
 */
export interface RuntimeView {
  /** The component of the runtime, e.g. `java-runtime-epsilon`. */
  component: string;
  /** The version of the runtime, e.g. `21.0.7`. */
  version: string;
  /** Whether the runtime is on the disk already. */
  installed: boolean;
  /** Where the runtime lives. */
  path: string;
}

/** What the frontend gets after a successful download of a runtime. */
export interface RuntimeResult {
  component: string;
  files: number;
  bytes: number;
  elapsed_ms: number;
}

/**
 * What the launcher tells the frontend while it downloads a runtime.
 *
 * The four kinds are the same four moments as the installation of a
 * version has.
 */
export type RuntimeEvent =
  | { kind: "started"; component: string; total: number; bytes: number }
  | { kind: "progress"; component: string; completed: number; total: number; bytes: number }
  | { kind: "finished"; component: string; files: number; bytes: number; elapsed_ms: number }
  | { kind: "failed"; component: string; message: string };

/**
 * The manifest of the versions, as Mojang publishes it.
 *
 * Only the fields the launcher reads are described. The rest of the
 * document is ignored on both sides.
 */
export interface VersionManifest {
  latest: { release: string; snapshot: string };
  versions: ManifestVersion[];
}

/** One version in the manifest. */
export interface ManifestVersion {
  id: string;
  type: string;
  url: string;
  time: string;
  releaseTime: string;
  sha1: string;
}
