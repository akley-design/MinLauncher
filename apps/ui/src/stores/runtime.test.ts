import { beforeEach, describe, expect, it } from "vitest";

import { applyRuntimeEvent, progressOf, useRuntimeStore } from "./runtime";

/**
 * The state of a download of a Java runtime.
 *
 * The state changes on its own, when the backend sends an event. The
 * events are the only thing that can move the bar, and the interface
 * has to believe them — so the transition is worth a test.
 */
describe("the state of a download of a runtime", () => {
  beforeEach(() => {
    useRuntimeStore.setState({
      component: null,
      completed: 0,
      total: 0,
      bytes: 0,
      error: null,
    });
  });

  it("starts empty", () => {
    expect(useRuntimeStore.getState().component).toBeNull();
    expect(useRuntimeStore.getState().completed).toBe(0);
  });

  it("remembers the runtime it is downloading", () => {
    applyRuntimeEvent(
      { kind: "started", component: "java-runtime-delta", total: 400, bytes: 5000 },
      useRuntimeStore,
    );

    const state = useRuntimeStore.getState();
    expect(state.component).toBe("java-runtime-delta");
    expect(state.total).toBe(400);
    expect(state.bytes).toBe(5000);
  });

  it("moves the bar forward", () => {
    applyRuntimeEvent(
      { kind: "started", component: "java-runtime-delta", total: 400, bytes: 5000 },
      useRuntimeStore,
    );
    applyRuntimeEvent(
      {
        kind: "progress",
        component: "java-runtime-delta",
        completed: 100,
        total: 400,
        bytes: 2500,
      },
      useRuntimeStore,
    );

    const state = useRuntimeStore.getState();
    expect(state.completed).toBe(100);
    expect(progressOf(state)).toBeCloseTo(0.25);
  });

  it("forgets the runtime when the download is over", () => {
    applyRuntimeEvent(
      { kind: "started", component: "java-runtime-delta", total: 400, bytes: 5000 },
      useRuntimeStore,
    );
    applyRuntimeEvent(
      {
        kind: "finished",
        component: "java-runtime-delta",
        files: 400,
        bytes: 5000,
        elapsed_ms: 1234,
      },
      useRuntimeStore,
    );

    const state = useRuntimeStore.getState();
    expect(state.component).toBeNull();
    expect(state.error).toBeNull();
  });

  it("remembers the failure of a download", () => {
    applyRuntimeEvent(
      { kind: "failed", component: "java-runtime-delta", message: "the network is down" },
      useRuntimeStore,
    );

    const state = useRuntimeStore.getState();
    expect(state.component).toBeNull();
    expect(state.error).toBe("the network is down");
  });

  it("calls a download that is not running done", () => {
    // Nothing is being downloaded, so there is nothing to wait for: the
    // bar is full and not stuck at zero.
    expect(progressOf(useRuntimeStore.getState())).toBe(1);
  });

  it("never goes past the end of the bar", () => {
    // The backend counts the files it finished, and a file that failed
    // counts too — so the count can be higher than the total it gave
    // before. The bar has to stay inside its track.
    applyRuntimeEvent(
      { kind: "started", component: "java-runtime-delta", total: 10, bytes: 0 },
      useRuntimeStore,
    );
    applyRuntimeEvent(
      { kind: "progress", component: "java-runtime-delta", completed: 12, total: 10, bytes: 0 },
      useRuntimeStore,
    );

    expect(progressOf(useRuntimeStore.getState())).toBe(1);
  });
});
