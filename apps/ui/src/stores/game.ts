/**
 * The state of the running game.
 *
 * The game is the one thing of the launcher that changes on its own:
 * it starts, it ends, and the frontend has to know about both without
 * the player touching anything. The state therefore lives in a store
 * of its own, and not inside a component, so that the sidebar and the
 * play button see the same thing.
 */

import { create } from "zustand";

import { onGameEvent } from "../ipc/events";
import type { GameEvent, RunningGame } from "../ipc/types";

/** The state of the game and the functions that change it. */
interface GameState {
  /** The game that is running, if any. */
  running: RunningGame | null;
  /** Whether the game is being started right now. */
  starting: boolean;
  /** The number of the attempt that is coming, when the game fell over. */
  restarting: number | null;
  /** The last failure of a start or a stop. */
  error: string | null;
  /** The version that was running when it ended, for the message. */
  lastVersion: string | null;

  /** The game started. */
  started: (game: RunningGame) => void;
  /** The game fell over and is being started again. */
  willRestart: (attempt: number) => void;
  /** The game ended. */
  exited: (versionId: string, success: boolean, stopped: boolean) => void;
  /** A start or a stop failed. */
  failed: (message: string) => void;
  /** The start is under way. */
  beginStart: () => void;
  /** The start is over, one way or another. */
  endStart: () => void;
}

export const useGameStore = create<GameState>((set) => ({
  running: null,
  starting: false,
  restarting: null,
  error: null,
  lastVersion: null,

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
      // A game the player closed is not a failure, and the interface
      // must not say that it was.
      error: success || stopped ? null : `the game ${versionId} ended with an error`,
    }),

  failed: (message) => set({ starting: false, error: message }),

  beginStart: () => set({ starting: true, error: null }),

  endStart: () => set({ starting: false }),
}));

/**
 * Apply one event of the game to the store.
 *
 * The function is exported so that it can be tested without a backend.
 */
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

/**
 * Listen to the events of the game and keep the store in sync.
 *
 * The returned function stops the listening.
 */
export async function watchGame(): Promise<() => void> {
  const unlisten = await onGameEvent((event) => {
    applyGameEvent(event, useGameStore);
  });

  return unlisten;
}
