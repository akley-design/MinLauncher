/**
 * The events the backend emits.
 *
 * The launcher tells the frontend when the game starts and when it ends
 * through one event channel. The frontend listens on it and turns the
 * event into a state change.
 */

import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { GameEvent } from "./types";

/** The channel the game events arrive on. */
export const GAME_EVENT = "minlauncher://game";

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
