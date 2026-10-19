/**
 * The events the backend emits.
 *
 * The launcher tells the frontend when the game starts and when it ends
 * through one event channel, and how an installation is going through
 * another one. The frontend listens on them and turns the events into
 * state changes.
 */

import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { GameEvent, InstallEvent, RuntimeEvent } from "./types";

/** The channel the game events arrive on. */
export const GAME_EVENT = "minlauncher://game";

/** The channel the events of an installation arrive on. */
export const INSTALL_EVENT = "minlauncher://install";

/** The channel the events of a runtime download arrive on. */
export const RUNTIME_EVENT = "minlauncher://runtime";

/**
 * Listen to the events of the game.
 *
 * The returned function stops the listening. It has to be called when
 * the component that listens is unmounted, otherwise the listener stays
 * and every event is handled twice.
 */
export async function onGameEvent(handler: (event: GameEvent) => void): Promise<UnlistenFn> {
  const unlisten = await listen<GameEvent>(GAME_EVENT, (message) => {
    handler(message.payload);
  });

  return unlisten;
}

/**
 * Listen to the events of an installation.
 *
 * The returned function stops the listening, in the same way as the
 * listening of the game.
 */
export async function onInstallEvent(handler: (event: InstallEvent) => void): Promise<UnlistenFn> {
  const unlisten = await listen<InstallEvent>(INSTALL_EVENT, (message) => {
    handler(message.payload);
  });

  return unlisten;
}

/**
 * Listen to the events of a runtime download.
 *
 * The returned function stops the listening, in the same way as the
 * listening of the installation.
 */
export async function onRuntimeEvent(handler: (event: RuntimeEvent) => void): Promise<UnlistenFn> {
  const unlisten = await listen<RuntimeEvent>(RUNTIME_EVENT, (message) => {
    handler(message.payload);
  });

  return unlisten;
}
