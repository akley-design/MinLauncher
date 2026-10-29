/** Event channels the backend emits on. */

import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { GameEvent, InstallEvent, InstallRootEvent, RuntimeEvent } from "./types";

export const GAME_EVENT = "minlauncher://game";
export const INSTALL_EVENT = "minlauncher://install";
export const RUNTIME_EVENT = "minlauncher://runtime";
export const INSTALL_ROOT_EVENT = "minlauncher://install-root";

/** Call the returned function on unmount, or every event is handled twice. */
export async function onGameEvent(handler: (event: GameEvent) => void): Promise<UnlistenFn> {
  const unlisten = await listen<GameEvent>(GAME_EVENT, (message) => {
    handler(message.payload);
  });

  return unlisten;
}

export async function onInstallEvent(handler: (event: InstallEvent) => void): Promise<UnlistenFn> {
  const unlisten = await listen<InstallEvent>(INSTALL_EVENT, (message) => {
    handler(message.payload);
  });

  return unlisten;
}

export async function onRuntimeEvent(handler: (event: RuntimeEvent) => void): Promise<UnlistenFn> {
  const unlisten = await listen<RuntimeEvent>(RUNTIME_EVENT, (message) => {
    handler(message.payload);
  });

  return unlisten;
}

export async function onInstallRootEvent(
  handler: (event: InstallRootEvent) => void,
): Promise<UnlistenFn> {
  const unlisten = await listen<InstallRootEvent>(INSTALL_ROOT_EVENT, (message) => {
    handler(message.payload);
  });

  return unlisten;
}
