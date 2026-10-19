import { beforeEach, describe, expect, it } from "vitest";

import { applyInstallEvent, progressOf, useInstallStore } from "./install";

/**
 * The state of an installation.
 *
 * The state changes on its own, when the backend sends an event. The
 * events are the only thing that can move the bar, and the interface
 * has to believe them — so the transition is worth a test.
 */
describe("the state of an installation", () => {
  beforeEach(() => {
    useInstallStore.setState({
      versionId: null,
      completed: 0,
      total: 0,
      bytes: 0,
      error: null,
      lastVersion: null,
    });
  });

  it("starts empty", () => {
    expect(useInstallStore.getState().versionId).toBeNull();
    expect(useInstallStore.getState().completed).toBe(0);
    expect(useInstallStore.getState().total).toBe(0);
  });

  it("remembers the version it is installing", () => {
    applyInstallEvent(
      { kind: "started", version_id: "26.3", total: 100, bytes: 500 },
      useInstallStore,
    );

    const state = useInstallStore.getState();
    expect(state.versionId).toBe("26.3");
    expect(state.total).toBe(100);
    expect(state.bytes).toBe(500);
  });

  it("moves the bar forward", () => {
    applyInstallEvent(
      { kind: "started", version_id: "26.3", total: 100, bytes: 500 },
      useInstallStore,
    );
    applyInstallEvent(
      { kind: "progress", version_id: "26.3", completed: 25, total: 100, bytes: 250 },
      useInstallStore,
    );

    const state = useInstallStore.getState();
    expect(state.completed).toBe(25);
    expect(state.bytes).toBe(250);
    expect(progressOf(state)).toBeCloseTo(0.25);
  });

  it("forgets the version when the installation is over", () => {
    applyInstallEvent(
      { kind: "started", version_id: "26.3", total: 100, bytes: 500 },
      useInstallStore,
    );
    applyInstallEvent(
      { kind: "finished", version_id: "26.3", files: 100, bytes: 500, elapsed_ms: 1234 },
      useInstallStore,
    );

    const state = useInstallStore.getState();
    expect(state.versionId).toBeNull();
    expect(state.error).toBeNull();
    expect(state.lastVersion).toBe("26.3");
  });

  it("remembers the failure of an installation", () => {
    applyInstallEvent(
      { kind: "failed", version_id: "26.3", message: "the network is down" },
      useInstallStore,
    );

    const state = useInstallStore.getState();
    expect(state.versionId).toBeNull();
    expect(state.error).toBe("the network is down");
  });

  it("calls a finished installation done", () => {
    // Nothing is being installed, so there is nothing to wait for: the
    // bar is full and not stuck at zero.
    expect(progressOf(useInstallStore.getState())).toBe(1);
  });

  it("calls a version that is not known yet not done", () => {
    // The installation has begun, but the backend did not say yet how
    // many files there are. The bar is empty, not full.
    useInstallStore.getState().began("26.3", 0, 0);
    expect(progressOf(useInstallStore.getState())).toBe(0);
  });

  it("never goes past the end of the bar", () => {
    // The backend counts the files it finished, and a file that failed
    // counts too — so the count can be higher than the total it gave
    // before. The bar has to stay inside its track.
    applyInstallEvent(
      { kind: "started", version_id: "26.3", total: 10, bytes: 0 },
      useInstallStore,
    );
    applyInstallEvent(
      { kind: "progress", version_id: "26.3", completed: 12, total: 10, bytes: 0 },
      useInstallStore,
    );

    expect(progressOf(useInstallStore.getState())).toBe(1);
  });
});
