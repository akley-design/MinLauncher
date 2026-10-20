/**
 * The keys of the queries.
 *
 * Every key is built by a function, so that a key cannot be written
 * twice in two different places and drift apart. TanStack Query uses
 * the key to decide what to invalidate, and a key that is written by
 * hand is a key that is invalidated by hand — and eventually one of the
 * two is forgotten.
 */

import type { AccountList, InstanceList, SettingsView, VersionList } from "./types";

/** The keys of the queries of the launcher. */
export const queryKeys = {
  versions: () => ["versions"] as const,
  manifest: () => ["manifest"] as const,
  accounts: () => ["accounts"] as const,
  instances: () => ["instances"] as const,
  settings: () => ["settings"] as const,
  runtimes: () => ["runtimes"] as const,
  downloads: () => ["downloads"] as const,
  benchmarks: () => ["benchmarks"] as const,
  benchmarkSeries: (metric: string, scenario: string) => ["benchmarks", metric, scenario] as const,
} as const;

/** The shape of the cached data, used by the invalidation helpers. */
export type VersionQueryData = VersionList;
export type AccountQueryData = AccountList;
export type InstanceQueryData = InstanceList;
export type SettingsQueryData = SettingsView;
