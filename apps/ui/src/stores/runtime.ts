/** One Java runtime download at a time, because the backend installs one per command. */

import { create } from "zustand";

import { onRuntimeEvent } from "../ipc/events";
import type { RuntimeEvent } from "../ipc/types";

interface RuntimeState {
  component: string | null;
  completed: number;
  total: number;
  bytes: number;
  error: string | null;

  began: (component: string, total: number, bytes: number) => void;
  progressed: (completed: number, total: number, bytes: number) => void;
  ended: () => void;
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

/** Progress between 0 and 1. Nothing downloading means done. */
export function progressOf(state: RuntimeState): number {
  if (state.total === 0) {
    return state.component === null ? 1 : 0;
  }
  return Math.min(1, state.completed / state.total);
}

/** Exported so the store can be tested without a backend. */
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

/** The returned function stops the listener. */
export async function watchRuntime(): Promise<() => void> {
  const unlisten = await onRuntimeEvent((event) => {
    applyRuntimeEvent(event, useRuntimeStore);
  });

  return unlisten;
}
