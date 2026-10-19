import { describe, expect, it } from "vitest";

import { CommandError, toCommandError } from "./client";

/**
 * The failure of a command.
 *
 * The backend reports its failures as an object with a code, and the
 * frontend switches on that code. The bridge itself can fail in other
 * ways, and those have to become the same shape — otherwise every
 * caller needs two error paths, and one of them is always forgotten.
 */
describe("the failure of a command", () => {
  it("keeps the code of a failure of the backend", () => {
    const error = toCommandError({
      code: "not_found",
      message: "there is no version `26.3`",
      detail: "the database is empty",
    });

    expect(error).toBeInstanceOf(CommandError);
    expect(error.code).toBe("not_found");
    expect(error.message).toBe("there is no version `26.3`");
    expect(error.detail).toBe("the database is empty");
  });

  it("turns a failure of the bridge into an internal one", () => {
    const error = toCommandError(new Error("the window was closed"));

    expect(error.code).toBe("internal");
    expect(error.message).toBe("the window was closed");
  });

  it("turns something that is not an error into an internal one", () => {
    const error = toCommandError("nonsense");

    expect(error.code).toBe("internal");
    expect(error.message).toBe("nonsense");
  });

  it("does not wrap a failure twice", () => {
    const original = new CommandError("network", "no route to host");
    expect(toCommandError(original)).toBe(original);
  });
});
