/**
 * The state of an installation.
 *
 * Installing a version is the one operation of the launcher that takes
 * minutes rather than seconds, and the player has to see what is
 * happening: how many files are done, how many bytes have moved, and
 * whether it failed. The state therefore lives in a store of its own,
 * the same way the state of the game does.
 *
 * Only one installation runs at a time — the backend installs one
 * version per command, and the launcher does not start a second one
 * while the first is going on.
 */

import { create } from "zustand";

import { onInstallEvent } from "../ipc/events";
import type { InstallEvent } from "../ipc/types";

/** The state of an installation and the functions that change it. */
interface InstallState {
  /** The version that is being installed, if any. */
  versionId: string | null;
  /** How many files are done. */
  completed: number;
  /** How many files there are in total. */
  total: number;
  /** How many bytes have been transferred. */
  bytes: number;
  /** The last failure of an installation. */
  error: string | null;
  /** The version that was installed last, for the message. */
  lastVersion: string | null;

  /** The installation started. */
  began: (versionId: string, total: number, bytes: number) => void;
  /** Some of the files are done. */
  progressed: (completed: number, total: number, bytes: number) => void;
  /** The installation ended. */
  ended: (versionId: string) => void;
  /** The installation failed. */
  failed: (message: string) => void;
}

export const useInstallStore = create<InstallState>((set) => ({
  versionId: null,
  completed: 0,
  total: 0,
  bytes: 0,
  error: null,
  lastVersion: null,

  began: (versionId, total, bytes) =>
    set({
      versionId,
      completed: 0,
      total,
      bytes,
      error: null,
      lastVersion: null,
    }),

  progressed: (completed, total, bytes) => set({ completed, total, bytes }),

  ended: (versionId) =>
    set({
      versionId: null,
      completed: 0,
      total: 0,
      bytes: 0,
      error: null,
      lastVersion: versionId,
    }),

  failed: (message) => set({ versionId: null, error: message }),
}));

/**
 * The part of the installation that is done, between 0 and 1.
 *
 * A version that is not being installed is done: there is nothing left
 * to wait for.
 */
export function progressOf(state: InstallState): number {
  if (state.total === 0) {
    return state.versionId === null ? 1 : 0;
  }
  return Math.min(1, state.completed / state.total);
}

/**
 * Apply one event of the installation to the store.
 *
 * The function is exported so that it can be tested without a backend.
 */
export function applyInstallEvent(event: InstallEvent, store: typeof useInstallStore): void {
  if (event.kind === "started") {
    store.getState().began(event.version_id, event.total, event.bytes);
    return;
  }

  if (event.kind === "progress") {
    store.getState().progressed(event.completed, event.total, event.bytes);
    return;
  }

  if (event.kind === "finished") {
    store.getState().ended(event.version_id);
    return;
  }

  store.getState().failed(event.message);
}

/**
 * Listen to the events of the installation and keep the store in sync.
 *
 * The returned function stops the listening.
 */
export async function watchInstall(): Promise<() => void> {
  const unlisten = await onInstallEvent((event) => {
    applyInstallEvent(event, useInstallStore);
  });

  return unlisten;
}
