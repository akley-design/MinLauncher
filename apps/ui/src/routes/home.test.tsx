/**
 * The screen the launcher opens on.
 *
 * The screen answers one question — "what do I play, and how do I start
 * it" — and the answer has to be the version the player picked, not the
 * one the launcher would have chosen on its own. These tests hold that
 * line: the list of the installed versions is a list, and the button
 * starts the version that is picked in it.
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { AccountList, InstanceList, LaunchResult, VersionList } from "../ipc/types";
import { HomeRoute } from "./home";

// The backend is replaced by a mock, so that the test does not need a
// running desktop application.
const listVersions = vi.fn<() => Promise<VersionList>>();
const listAccounts = vi.fn<() => Promise<AccountList>>();
const listInstances = vi.fn<() => Promise<InstanceList>>();
const launchGame = vi.fn<(options: { version_id: string }) => Promise<LaunchResult>>();

vi.mock("../ipc/client", async () => {
  const actual = await vi.importActual<typeof import("../ipc/client")>("../ipc/client");
  return {
    ...actual,
    listVersions: () => listVersions(),
    listAccounts: () => listAccounts(),
    listInstances: () => listInstances(),
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

/** One account, and it is the selected one. */
function accounts(): AccountList {
  return {
    accounts: [
      {
        id: "acc-1",
        uuid: "1234",
        username: "Steve",
        kind: "offline",
        uuid_prefix: "1234",
        selected: true,
        created_at: "2026-10-20T10:00:00Z",
      },
    ],
    selected_id: "acc-1",
  };
}

/** The screen, with a fresh query cache around it. */
function renderScreen() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });

  return render(
    <QueryClientProvider client={client}>
      <HomeRoute />
    </QueryClientProvider>,
  );
}

describe("the screen of the launch", () => {
  beforeEach(() => {
    listVersions.mockReset();
    listAccounts.mockReset();
    listInstances.mockReset();
    launchGame.mockReset();
    listVersions.mockResolvedValue(versions());
    listAccounts.mockResolvedValue(accounts());
    listInstances.mockResolvedValue({ instances: [] } as InstanceList);
    launchGame.mockResolvedValue({
      game: { pid: 1, version_id: "1.12.2", command: "java", started_at: 0 },
      prepare_ms: 700,
    });
  });

  it("offers every installed version and starts the one that is picked", async () => {
    renderScreen();

    const picker = await screen.findByLabelText("Версия");
    expect(picker).toHaveValue("26.3");

    await userEvent.selectOptions(picker, "1.12.2");
    expect(picker).toHaveValue("1.12.2");

    await userEvent.click(screen.getByRole("button", { name: "Играть" }));

    await waitFor(() => expect(launchGame).toHaveBeenCalledTimes(1));
    expect(launchGame.mock.calls[0]?.[0]).toEqual({ version_id: "1.12.2" });
  });

  it("does not offer a version that is not installed", async () => {
    renderScreen();

    const picker = await screen.findByLabelText("Версия");
    const options = Array.from(picker.querySelectorAll("option")).map((option) => option.value);

    expect(options).toEqual(["26.3", "1.12.2"]);
  });
});
