/** Query keys and the mutation that starts the game. A key is built once and reused. */

import { useMutation, useQueryClient } from "@tanstack/react-query";

import { useGameStore } from "../stores/game";
import { launchGame, toCommandError } from "./client";

/** The keys of the queries of the launcher. */
export const queryKeys = {
  versions: () => ["versions"] as const,
  modloaders: () => ["modloaders"] as const,
  manifest: () => ["manifest"] as const,
  accounts: () => ["accounts"] as const,
  instances: () => ["instances"] as const,
  settings: () => ["settings"] as const,
  runtimes: () => ["runtimes"] as const,
  downloads: () => ["downloads"] as const,
  benchmarks: () => ["benchmarks"] as const,
  inGameComparison: () => ["benchmarks", "in-game"] as const,
  launcherUpdate: () => ["launcher", "update"] as const,
} as const;

/** Start the game. The start state lives in the store because several screens show it. */
export function useLaunchGame() {
  const queryClient = useQueryClient();
  const beginStart = useGameStore((state) => state.beginStart);
  const endStart = useGameStore((state) => state.endStart);
  const failed = useGameStore((state) => state.failed);
  const prepared = useGameStore((state) => state.prepared);

  return useMutation({
    mutationFn: (request: string | import("./types").LaunchOptions) =>
      launchGame(typeof request === "string" ? { version_id: request } : request),
    onMutate: () => beginStart(),
    onSuccess: (result) => {
      endStart();
      prepared(result.game.version_id, result.prepare_ms);
      void queryClient.invalidateQueries({ queryKey: queryKeys.benchmarks() });
    },
    onError: (error) => failed(toCommandError(error).message),
  });
}
