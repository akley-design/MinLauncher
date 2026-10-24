import { describe, expect, it } from "vitest";

import { parseMinLauncherUrl } from "./auth";

describe("parseMinLauncherUrl", () => {
  it("accepts an OAuth code only from the registered callback", () => {
    expect(parseMinLauncherUrl("minlauncher://auth/callback?code=pkce-code")).toEqual({
      kind: "oauth",
      code: "pkce-code",
    });
  });

  it("accepts website handoff codes separately", () => {
    expect(parseMinLauncherUrl("minlauncher://auth/transfer?code=one-time-code")).toEqual({
      kind: "transfer",
      code: "one-time-code",
    });
  });

  it("rejects unrelated schemes, paths and malformed input", () => {
    expect(parseMinLauncherUrl("https://auth/callback?code=x")).toBeNull();
    expect(parseMinLauncherUrl("minlauncher://other/callback?code=x")).toBeNull();
    expect(parseMinLauncherUrl("minlauncher://auth/other?code=x")).toBeNull();
    expect(parseMinLauncherUrl("not a URL")).toBeNull();
  });
});
