/**
 * The state of a download of a Java runtime.
 *
 * A runtime is a few hundred files and takes a while to fetch, so the
 * player has to see that something is happening. The state lives in a
 * store of its own, the same way the state of an installation does.
 *
 * Only one runtime is downloaded at a time — the backend installs one
 * runtime per command.
 */

import { create } from "zustand";

import { onRuntimeEvent } from "../ipc/events";
import type { RuntimeEvent } from "../ipc/types";

/** The state of a download and the functions that change it. */
interface RuntimeState {
  /** The runtime that is being downloaded, if any. */
  component: string | null;
  /** How many files are done. */
  completed: number;
  /** How many files there are in total. */
  total: number;
  /** How many bytes have been transferred. */
  bytes: number;
  /** The last failure of a download. */
  error: string | null;

  /** The download started. */
  began: (component: string, total: number, bytes: number) => void;
  /** Some of the files are done. */
  progressed: (completed: number, total: number, bytes: number) => void;
  /** The download ended. */
  ended: () => void;
  /** The download failed. */
  failed: (message: string) => void;
}

export const useRuntimeStore = create<RuntimeState>((set) => ({
  component: null,
  completed: 0,
  total: 0,
  bytes: 0,
  error: null,

  began: (component, total, bytes) =>
    set({
      component,
      completed: 0,
      total,
      bytes,
      error: null,
    }),

  progressed: (completed, total, bytes) => set({ completed, total, bytes }),

  ended: () => set({ component: null, completed: 0, total: 0, bytes: 0, error: null }),

  failed: (message) => set({ component: null, error: message }),
}));

/**
 * The part of the download that is done, between 0 and 1.
 *
 * A runtime that is not being downloaded is done: there is nothing left
 * to wait for.
 */
export function progressOf(state: RuntimeState): number {
  if (state.total === 0) {
    return state.component === null ? 1 : 0;
  }
  return Math.min(1, state.completed / state.total);
}

/**
 * Apply one event of the download to the store.
 *
 * The function is exported so that it can be tested without a backend.
 */
export function applyRuntimeEvent(event: RuntimeEvent, store: typeof useRuntimeStore): void {
  if (event.kind === "started") {
    store.getState().began(event.component, event.total, event.bytes);
    return;
  }

  if (event.kind === "progress") {
    store.getState().progressed(event.completed, event.total, event.bytes);
    return;
  }

  if (event.kind === "finished") {
    store.getState().ended();
    return;
  }

  store.getState().failed(event.message);
}

/**
 * Listen to the events of the download and keep the store in sync.
 *
 * The returned function stops the listening.
 */
export async function watchRuntime(): Promise<() => void> {
  const unlisten = await onRuntimeEvent((event) => {
    applyRuntimeEvent(event, useRuntimeStore);
  });

  return unlisten;
}
