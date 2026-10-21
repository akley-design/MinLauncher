/**
 * The screen of the versions.
 *
 * The list is the place where the player looks for the version they
 * want to play, and the button of the row has to start that very
 * version — not the one the launcher would have picked on its own.
 * These tests hold that line.
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { LaunchResult, VersionList } from "../ipc/types";
import { VersionsRoute } from "./versions";

// The backend is replaced by a mock, so that the test does not need a
// running desktop application.
const listVersions = vi.fn<() => Promise<VersionList>>();
const installVersion = vi.fn<(options: { version_id: string }) => Promise<unknown>>();
const refreshManifest = vi.fn<() => Promise<unknown>>();
const launchGame = vi.fn<(options: { version_id: string }) => Promise<LaunchResult>>();

vi.mock("../ipc/client", async () => {
  const actual = await vi.importActual<typeof import("../ipc/client")>("../ipc/client");
  return {
    ...actual,
    listVersions: () => listVersions(),
    installVersion: (options: { version_id: string }) => installVersion(options),
    refreshManifest: () => refreshManifest(),
    launchGame: (options: { version_id: string }) => launchGame(options),
  };
});

/** The list of the versions, with two of them installed. */
function versions(): VersionList {
  return {
    versions: [
      { id: "26.3", kind: "release", installed: true, java_major: 25 },
      { id: "1.12.2", kind: "release", installed: true, java_major: 8 },
      { id: "26.4-snapshot-5", kind: "snapshot", installed: false },
    ],
    latest_release: "26.3",
    latest_snapshot: "26.4-snapshot-5",
    installed_count: 2,
  };
}

/** The screen, with a fresh query cache around it. */
function renderScreen() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });

  return render(
    <QueryClientProvider client={client}>
      <VersionsRoute />
    </QueryClientProvider>,
  );
}

describe("the screen of the versions", () => {
  beforeEach(() => {
    listVersions.mockReset();
    installVersion.mockReset();
    refreshManifest.mockReset();
    launchGame.mockReset();
    listVersions.mockResolvedValue(versions());
    launchGame.mockResolvedValue({
      game: { pid: 1, version_id: "1.12.2", command: "java", started_at: 0 },
      prepare_ms: 700,
    });
  });

  it("starts the version of the row the button is in", async () => {
    renderScreen();

    const row = await screen.findByRole("row", { name: /1\.12\.2/ });
    await userEvent.click(within(row).getByRole("button", { name: "Играть" }));

    await waitFor(() => expect(launchGame).toHaveBeenCalledTimes(1));
    expect(launchGame.mock.calls[0]?.[0]).toEqual({ version_id: "1.12.2" });
  });

  it("offers to install a version that is not installed, not to play it", async () => {
    renderScreen();

    const row = await screen.findByRole("row", { name: /26\.4-snapshot-5/ });
    expect(within(row).getByRole("button", { name: "Установить" })).toBeInTheDocument();
    expect(within(row).queryByRole("button", { name: "Играть" })).not.toBeInTheDocument();
  });
});
