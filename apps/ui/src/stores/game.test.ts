import { beforeEach, describe, expect, it } from "vitest";

import { applyGameEvent, useGameStore } from "./game";

/**
 * The state of the game.
 *
 * The state changes on its own, when the backend sends an event. The
 * events are the only thing that can start or stop the game, and the
 * interface has to believe them — so the transition is worth a test.
 */
describe("the state of the game", () => {
  beforeEach(() => {
    useGameStore.setState({
      running: null,
      starting: false,
      error: null,
      lastVersion: null,
    });
  });

  it("starts empty", () => {
    expect(useGameStore.getState().running).toBeNull();
    expect(useGameStore.getState().starting).toBe(false);
  });

  it("remembers the game that was started", () => {
    const game = {
      pid: 4242,
      version_id: "26.3",
      command: "java -version 25 Main",
      started_at: 1_700_000_000_000,
    };

    applyGameEvent({ kind: "started", game }, useGameStore);

    const state = useGameStore.getState();
    expect(state.running).toEqual(game);
    expect(state.starting).toBe(false);
    expect(state.lastVersion).toBe("26.3");
  });

  it("forgets the game when it ended", () => {
    useGameStore.setState({
      running: {
        pid: 1,
        version_id: "26.3",
        command: "",
        started_at: 0,
      },
    });

    applyGameEvent({ kind: "exited", version_id: "26.3", success: true, code: 0 }, useGameStore);

    const state = useGameStore.getState();
    expect(state.running).toBeNull();
    expect(state.error).toBeNull();
  });

  it("reports the failure when the game ended badly", () => {
    applyGameEvent({ kind: "exited", version_id: "26.3", success: false, code: 1 }, useGameStore);

    const state = useGameStore.getState();
    expect(state.running).toBeNull();
    expect(state.error).toContain("26.3");
  });

  it("remembers the failure of a start", () => {
    useGameStore.getState().beginStart();
    expect(useGameStore.getState().starting).toBe(true);

    useGameStore.getState().failed("there is no java runtime");
    expect(useGameStore.getState().starting).toBe(false);
    expect(useGameStore.getState().error).toBe("there is no java runtime");
  });
});
