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

/** The launcher's complete settings. */
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

/** What the launcher tells the frontend while the game runs. */
export type GameEvent =
  | { kind: "started"; game: RunningGame }
  | { kind: "exited"; version_id: string; success: boolean; code: number | null };

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
