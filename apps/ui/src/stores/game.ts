/** The running game. It changes without input, so the state lives outside components. */

import { create } from "zustand";

import { onGameEvent } from "../ipc/events";
import type { GameEvent, RunningGame } from "../ipc/types";
import type { LaunchRecord } from "../lib/launches";

export const MAX_LAUNCHES = 20;

interface GameState {
  running: RunningGame | null;
  starting: boolean;
  restarting: number | null;
  error: string | null;
  lastVersion: string | null;
  launches: LaunchRecord[];

  started: (game: RunningGame) => void;
  willRestart: (attempt: number) => void;
  exited: (versionId: string, success: boolean, stopped: boolean) => void;
  failed: (message: string) => void;
  beginStart: () => void;
  endStart: () => void;
  prepared: (versionId: string, prepareMs: number) => void;
}

export const useGameStore = create<GameState>((set) => ({
  running: null,
  starting: false,
  restarting: null,
  error: null,
  lastVersion: null,
  launches: [],

  started: (game) =>
    set({
      running: game,
      starting: false,
      restarting: null,
      error: null,
      lastVersion: game.version_id,
    }),

  willRestart: (attempt) => set({ restarting: attempt }),

  exited: (versionId, success, stopped) =>
    set({
      running: null,
      starting: false,
      restarting: null,
      lastVersion: versionId,
      error: success || stopped ? null : `the game ${versionId} ended with an error`,
    }),

  failed: (message) => set({ starting: false, error: message }),

  beginStart: () => set({ starting: true, error: null }),

  endStart: () => set({ starting: false }),

  prepared: (versionId, prepareMs) =>
    set((state) => ({
      launches: [{ versionId, prepareMs, at: Date.now() }, ...state.launches].slice(
        0,
        MAX_LAUNCHES,
      ),
    })),
}));

/** Exported so the store can be tested without a backend. */
export function applyGameEvent(event: GameEvent, store: typeof useGameStore): void {
  if (event.kind === "started") {
    store.getState().started(event.game);
    return;
  }

  if (event.kind === "restarting") {
    store.getState().willRestart(event.attempt);
    return;
  }

  store.getState().exited(event.version_id, event.success, event.stopped);
}

/** The returned function stops the listener. */
export async function watchGame(): Promise<() => void> {
  const unlisten = await onGameEvent((event) => {
    applyGameEvent(event, useGameStore);
  });

  return unlisten;
}
