/**
 * The launches the launcher prepared.
 *
 * The gathering is the arithmetic of the panel of the performance: the
 * versions come together, and the middle of the preparations of a
 * version is the number that is compared with the promise.
 */

import { describe, expect, it } from "vitest";

import { summarizeLaunches } from "./launches";

describe("the launches of the launcher", () => {
  it("gathers the launches of every version and finds the middle one", () => {
    const summary = summarizeLaunches([
      { versionId: "26.3", prepareMs: 900, at: 3 },
      { versionId: "26.3", prepareMs: 700, at: 2 },
      { versionId: "26.3", prepareMs: 800, at: 1 },
      { versionId: "1.12.2", prepareMs: 1200, at: 0 },
    ]);

    expect(summary).toHaveLength(2);
    expect(summary[0]?.versionId).toBe("26.3");
    expect(summary[0]?.count).toBe(3);
    expect(summary[0]?.median).toBe(800);
    expect(summary[0]?.fastest).toBe(700);
    expect(summary[0]?.slowest).toBe(900);
    expect(summary[1]?.versionId).toBe("1.12.2");
    expect(summary[1]?.median).toBe(1200);
  });

  it("takes the mean of the two middle ones when the count is even", () => {
    const summary = summarizeLaunches([
      { versionId: "26.3", prepareMs: 700, at: 2 },
      { versionId: "26.3", prepareMs: 900, at: 1 },
    ]);

    expect(summary[0]?.median).toBe(800);
  });

  it("has nothing to say about a session without launches", () => {
    expect(summarizeLaunches([])).toEqual([]);
  });
});
