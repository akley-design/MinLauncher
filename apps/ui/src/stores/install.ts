/** One installation at a time, because the backend installs one version per command. */

import { create } from "zustand";

import { onInstallEvent } from "../ipc/events";
import type { InstallEvent } from "../ipc/types";

interface InstallState {
  versionId: string | null;
  completed: number;
  total: number;
  bytes: number;
  error: string | null;
  lastVersion: string | null;

  began: (versionId: string, total: number, bytes: number) => void;
  progressed: (completed: number, total: number, bytes: number) => void;
  ended: (versionId: string) => void;
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

/** Progress between 0 and 1. Nothing installing means done. */
export function progressOf(state: InstallState): number {
  if (state.total === 0) {
    return state.versionId === null ? 1 : 0;
  }
  return Math.min(1, state.completed / state.total);
}

/** Exported so the store can be tested without a backend. */
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

/** The returned function stops the listener. */
export async function watchInstall(): Promise<() => void> {
  const unlisten = await onInstallEvent((event) => {
    applyInstallEvent(event, useInstallStore);
  });

  return unlisten;
}
