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

import type {
  AccountList,
  Instance,
  InstanceList,
  InstallResult,
  LaunchResult,
  VersionList,
} from "../ipc/types";
import { useGameStore } from "../stores/game";
import { HomeRoute } from "./home";

// The backend is replaced by a mock, so that the test does not need a
// running desktop application.
const listVersions = vi.fn<() => Promise<VersionList>>();
const listAccounts = vi.fn<() => Promise<AccountList>>();
const listInstances = vi.fn<() => Promise<InstanceList>>();
const launchGame = vi.fn<(options: { version_id: string; instance_id?: string }) => Promise<LaunchResult>>();
const installVersion = vi.fn<(options: { version_id: string }) => Promise<InstallResult>>();

vi.mock("../ipc/client", async () => {
  const actual = await vi.importActual<typeof import("../ipc/client")>("../ipc/client");
  return {
    ...actual,
    listVersions: () => listVersions(),
    listAccounts: () => listAccounts(),
    listInstances: () => listInstances(),
    launchGame: (options: { version_id: string; instance_id?: string }) => launchGame(options),
    installVersion: (options: { version_id: string }) => installVersion(options),
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
    installVersion.mockReset();
    listVersions.mockResolvedValue(versions());
    listAccounts.mockResolvedValue(accounts());
    listInstances.mockResolvedValue({ instances: [] } as InstanceList);
    installVersion.mockResolvedValue({
      version_id: "26.3",
      files: 10,
      bytes: 10,
      elapsed_ms: 10,
    });
    // The backend answers with the version it was asked for, the way
    // the real one does.
    launchGame.mockImplementation(async (options) => ({
      game: { pid: 1, version_id: options.version_id, command: "java", started_at: 0 },
      prepare_ms: 700,
    }));
    // The store is shared by the whole tab, so it has to be emptied
    // between the tests.
    useGameStore.setState({ launches: [] });
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

  it("starts the selected isolated instance", async () => {
    const instance: Instance = {
      id: "forge-1201",
      version_id: "1.20.1-forge-47.4.20",
      name: "Forge 1.20.1 · MinLauncher",
      game_dir: "C:/MinLauncher/instances/forge-1201",
      java_path: null,
      jvm_args: null,
      created_at: "2026-10-22T10:00:00Z",
    };
    listInstances.mockResolvedValue({ instances: [instance] });

    renderScreen();
    const picker = await screen.findByLabelText("Версия");
    expect(picker).toHaveValue("instance:forge-1201");
    await userEvent.click(screen.getByRole("button", { name: "Играть" }));

    await waitFor(() => expect(launchGame).toHaveBeenCalledTimes(1));
    expect(launchGame.mock.calls[0]?.[0]).toEqual({
      version_id: "1.20.1-forge-47.4.20",
      instance_id: "forge-1201",
    });
  });

  it("offers to install a version when none is installed", async () => {
    listVersions.mockResolvedValue({
      versions: [{ id: "26.3", kind: "release", installed: false, java_major: 25 }],
      latest_release: "26.3",
      latest_snapshot: "26.3",
      installed_count: 0,
    });
    renderScreen();

    // Nothing is installed, and the player must not have to hunt for
    // the screen that installs one.
    const button = await screen.findByRole("button", { name: "Установить 26.3" });
    await userEvent.click(button);

    await waitFor(() => expect(installVersion).toHaveBeenCalledTimes(1));
    expect(installVersion.mock.calls[0]?.[0]).toEqual({ version_id: "26.3" });
  });

  it("keeps the preparation of the launch when the screen comes back", async () => {
    const { unmount } = renderScreen();
    await screen.findByLabelText("Версия");

    await userEvent.click(screen.getByRole("button", { name: "Играть" }));
    await waitFor(() => expect(launchGame).toHaveBeenCalledTimes(1));
    expect(await screen.findByText("Подготовка 26.3 заняла 700 мс")).toBeInTheDocument();

    // The player goes to another screen and comes back: the number of
    // the preparation is still there.
    unmount();
    renderScreen();

    expect(await screen.findByText("Подготовка 26.3 заняла 700 мс")).toBeInTheDocument();
  });
});
