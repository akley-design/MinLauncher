import { describe, expect, it } from "vitest";

import { formatTime, formatValue } from "./format";

/**
 * The time of a release.
 *
 * The value comes from the backend in ISO-8601 in UTC. It has to be
 * shown in the time zone of the reader, because a release date in UTC
 * is off by a day for half of the world.
 */
describe("the time of a release", () => {
  it("shows a day for a value that parses", () => {
    const shown = formatTime("2026-01-15T12:00:00+00:00");
    expect(shown).toMatch(/^\d{2}\.\d{2}\.\d{4}$/);
  });

  it("shows a dash when there is no time", () => {
    expect(formatTime(undefined)).toBe("—");
  });

  it("shows the value itself when it cannot be parsed", () => {
    // A malformed date is not worth crashing a table over, and hiding
    // it is worse than showing it: the player can see that something is
    // wrong with the data.
    expect(formatTime("вчера")).toBe("вчера");
  });
});

/**
 * A number with its unit.
 *
 * The table of the measurements has to be readable at a glance, so the
 * numbers are shortened and the unit is spelled out. A number with
 * twelve digits in it cannot be compared with another one at a glance,
 * and comparing the numbers is the whole point of the table.
 */
describe("a number with its unit", () => {
  it("shows the milliseconds as they are", () => {
    expect(formatValue(12.34, "ms")).toBe("12.3 мс");
  });

  it("shows the seconds when the milliseconds are too many", () => {
    expect(formatValue(2500, "ms")).toBe("2.50 с");
  });

  it("shows the frames per second as they are", () => {
    expect(formatValue(143.7, "fps")).toBe("143.7");
  });

  it("shows the megabytes", () => {
    expect(formatValue(58.25, "mb")).toBe("58.3 МБ");
  });

  it("shows the ratio as a percentage", () => {
    expect(formatValue(0.83, "ratio")).toBe("83.0 %");
  });

  it("shows an unknown unit as it is", () => {
    expect(formatValue(1.5, "widgets")).toBe("1.50 widgets");
  });
});
