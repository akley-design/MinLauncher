/**
 * The keys of the queries and the mutation that starts the game.
 *
 * Every key is built by a function, so that a key cannot be written
 * twice in two different places and drift apart. TanStack Query uses
 * the key to decide what to invalidate, and a key that is written by
 * hand is a key that is invalidated by hand — and eventually one of the
 * two is forgotten.
 */

import { useMutation, useQueryClient } from "@tanstack/react-query";

import { useGameStore } from "../stores/game";
import { launchGame, toCommandError } from "./client";
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
  inGameComparison: () => ["benchmarks", "in-game"] as const,
} as const;

/** The shape of the cached data, used by the invalidation helpers. */
export type VersionQueryData = VersionList;
export type AccountQueryData = AccountList;
export type InstanceQueryData = InstanceList;
export type SettingsQueryData = SettingsView;

/**
 * The mutation that starts the game.
 *
 * The state of the start lives in the store of the game and not in the
 * screen that asked for it: the sidebar and the button have to see the
 * same thing, and the screen that starts the game is not always the
 * screen that shows it — the game can be started from the list of the
 * versions as well.
 *
 * The mutation takes the version to play, because that is what the
 * player picks, and every version of the game can be played.
 */
export function useLaunchGame() {
  const queryClient = useQueryClient();
  const beginStart = useGameStore((state) => state.beginStart);
  const endStart = useGameStore((state) => state.endStart);
  const failed = useGameStore((state) => state.failed);

  return useMutation({
    mutationFn: (versionId: string) => launchGame({ version_id: versionId }),
    onMutate: () => beginStart(),
    onSuccess: (result) => {
      endStart();
      // The result carries the time the launcher spent preparing the
      // launch. It is the number M3 is built on, so it is worth
      // showing rather than throwing away.
      void queryClient.invalidateQueries({ queryKey: queryKeys.benchmarks() });
      void result;
    },
    onError: (error) => failed(toCommandError(error).message),
  });
}
