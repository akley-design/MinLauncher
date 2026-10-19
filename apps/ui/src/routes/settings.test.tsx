/**
 * The screen of the settings.
 *
 * The screen is the one place where the copy that the player edits and
 * the copy that is on the disk have to stay apart — and come together
 * again at the right moment. These tests hold that line: the form shows
 * what the backend sent, the edits stay local until the button is
 * pressed, and the button sends the whole thing back.
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { LauncherConfig, SettingsView } from "../ipc/types";
import { SettingsRoute } from "./settings";

// The backend is replaced by a mock, so that the test does not need a
// running desktop application. The mock is the only place where the
// test knows anything about the IPC.
const readSettings = vi.fn<() => Promise<SettingsView>>();
const writeSettings = vi.fn<(config: LauncherConfig) => Promise<void>>();

vi.mock("../ipc/client", async () => {
  const actual = await vi.importActual<typeof import("../ipc/client")>("../ipc/client");
  return {
    ...actual,
    readSettings: () => readSettings(),
    writeSettings: (config: LauncherConfig) => writeSettings(config),
  };
});

/** The settings the backend sends. */
function settings(overrides: Partial<LauncherConfig> = {}): SettingsView {
  const config: LauncherConfig = {
    schema_version: 1,
    language: "ru-RU",
    minimize_to_tray: true,
    autostart: false,
    default_version: null,
    default_jre: "auto",
    show_console: false,
    downloads: {
      max_concurrent: 8,
      max_retries: 3,
      timeout_secs: 30,
      verify_checksums: true,
      bandwidth_limit_kbps: 0,
    },
    jvm: {
      max_heap: "4G",
      min_heap: "1G",
      garbage_collector: "G1",
      extra_args: ["-XX:+UseStringDeduplication"],
      enable_agent: false,
    },
    telemetry_enabled: false,
    ...overrides,
  };

  return {
    config,
    directories: {
      data: "C:/Users/player/AppData/Roaming/MinLauncher",
      cache: "C:/Users/player/AppData/Local/MinLauncher/cache",
      database: "C:/Users/player/AppData/Roaming/MinLauncher/minlaucher.db",
    },
  };
}

/** The screen, with a fresh query cache around it. */
function renderScreen() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });

  return render(
    <QueryClientProvider client={client}>
      <SettingsRoute />
    </QueryClientProvider>,
  );
}

describe("the screen of the settings", () => {
  beforeEach(() => {
    readSettings.mockReset();
    writeSettings.mockReset();
    writeSettings.mockResolvedValue(undefined);
  });

  it("shows what the backend sent", async () => {
    readSettings.mockResolvedValue(settings());
    renderScreen();

    const heap = await screen.findByLabelText("Максимальная куча");
    expect(heap).toHaveValue("4G");

    expect(screen.getByLabelText("Минимальная куча")).toHaveValue("1G");
    expect(screen.getByLabelText("Сборщик мусора")).toHaveValue("G1");
    expect(screen.getByLabelText("Язык интерфейса")).toHaveValue("ru-RU");
    expect(screen.getByText("C:/Users/player/AppData/Roaming/MinLauncher")).toBeInTheDocument();
  });

  it("keeps the edits to itself until the button is pressed", async () => {
    readSettings.mockResolvedValue(settings());
    renderScreen();

    const heap = await screen.findByLabelText("Максимальная куча");
    await userEvent.clear(heap);
    await userEvent.type(heap, "8G");

    expect(writeSettings).not.toHaveBeenCalled();

    await userEvent.click(screen.getByRole("button", { name: "Сохранить" }));

    await waitFor(() => expect(writeSettings).toHaveBeenCalledTimes(1));
    expect(writeSettings.mock.calls[0]?.[0]?.jvm.max_heap).toBe("8G");
  });

  it("sends the whole settings back, not only what was changed", async () => {
    readSettings.mockResolvedValue(settings());
    renderScreen();

    await screen.findByLabelText("Максимальная куча");
    await userEvent.click(screen.getByRole("checkbox", { name: /Запускать вместе с системой/ }));
    await userEvent.click(screen.getByRole("button", { name: "Сохранить" }));

    await waitFor(() => expect(writeSettings).toHaveBeenCalledTimes(1));
    const sent = writeSettings.mock.calls[0]?.[0];
    expect(sent?.autostart).toBe(true);
    expect(sent?.minimize_to_tray).toBe(true);
    expect(sent?.jvm.garbage_collector).toBe("G1");
    expect(sent?.downloads.max_concurrent).toBe(8);
  });

  it("turns the extra arguments into a list of lines", async () => {
    readSettings.mockResolvedValue(settings());
    renderScreen();

    const args = await screen.findByLabelText("Дополнительные аргументы JVM");
    expect(args).toHaveValue("-XX:+UseStringDeduplication");

    await userEvent.clear(args);
    await userEvent.click(args);
    await userEvent.paste("-XX:+UseG1GC\n-XX:+UseStringDeduplication");

    await userEvent.click(screen.getByRole("button", { name: "Сохранить" }));

    await waitFor(() => expect(writeSettings).toHaveBeenCalledTimes(1));
    expect(writeSettings.mock.calls[0]?.[0]?.jvm.extra_args).toEqual([
      "-XX:+UseG1GC",
      "-XX:+UseStringDeduplication",
    ]);
  });

  it("shows the failure of the backend instead of the form", async () => {
    readSettings.mockRejectedValue({
      code: "storage",
      message: "the settings file is locked",
    });
    renderScreen();

    expect(await screen.findByText("the settings file is locked")).toBeInTheDocument();
    expect(screen.queryByLabelText("Максимальная куча")).not.toBeInTheDocument();
  });
});
