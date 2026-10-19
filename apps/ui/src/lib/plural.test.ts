import { describe, expect, it } from "vitest";

import { plural } from "./plural";

/**
 * The Russian plural.
 *
 * Russian has three plural forms, and the rule is not the one of
 * English: the form depends on the last digit and on the last two
 * digits together. A launcher that says "2 версия" instead of "2 версии"
 * reads as broken to every Russian speaker, so the rule is worth a
 * test of its own.
 */
describe("the Russian plural", () => {
  it("uses the first form for the numbers that end in one", () => {
    expect(plural(1, "версия", "версии", "версий")).toBe("версия");
    expect(plural(21, "версия", "версии", "версий")).toBe("версия");
    expect(plural(101, "версия", "версии", "версий")).toBe("версия");
  });

  it("uses the second form for the numbers that end in two, three or four", () => {
    expect(plural(2, "версия", "версии", "версий")).toBe("версии");
    expect(plural(3, "версия", "версии", "версий")).toBe("версии");
    expect(plural(4, "версия", "версии", "версий")).toBe("версии");
    expect(plural(22, "версия", "версии", "версий")).toBe("версии");
  });

  it("uses the third form for the rest", () => {
    expect(plural(0, "версия", "версии", "версий")).toBe("версий");
    expect(plural(5, "версия", "версии", "версий")).toBe("версий");
    expect(plural(11, "версия", "версии", "версий")).toBe("версий");
    expect(plural(12, "версия", "версии", "версий")).toBe("версий");
    expect(plural(14, "версия", "версии", "версий")).toBe("версий");
    expect(plural(25, "версия", "версии", "версий")).toBe("версий");
  });
});
