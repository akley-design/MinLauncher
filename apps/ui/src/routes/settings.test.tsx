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
const pickInstallDirectory = vi.fn<() => Promise<string | null>>();

vi.mock("../ipc/client", async () => {
  const actual = await vi.importActual<typeof import("../ipc/client")>("../ipc/client");
  return {
    ...actual,
    readSettings: () => readSettings(),
    writeSettings: (config: LauncherConfig) => writeSettings(config),
    pickInstallDirectory: () => pickInstallDirectory(),
    listRuntimes: () => Promise.resolve([]),
  };
});

vi.mock("../ipc/events", () => ({
  onInstallRootEvent: () => Promise.resolve(() => {}),
}));

/** The settings the backend sends. */
function settings(
  overrides: Partial<LauncherConfig> = {},
  autostartSupported = true,
): SettingsView {
  const config: LauncherConfig = {
    schema_version: 1,
    language: "ru-RU",
    minimize_to_tray: true,
    autostart: false,
    default_version: null,
    default_jre: "auto",
    install_dir: "",
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
      flags_profile: "default",
    },
    optimization: { profile: "max_fps" },
    mods: {
      enabled: true,
      loader_version: "0.16.9",
      mod_jar: "",
      preset: "balanced",
    },
    restart: {
      enabled: true,
      max_restarts: 3,
      backoff_ms: 5000,
      window_ms: 300_000,
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
      install: "",
      versions: "C:/Users/player/AppData/Roaming/MinLauncher/versions",
      libraries: "C:/Users/player/AppData/Local/MinLauncher/libraries",
      assets: "C:/Users/player/AppData/Local/MinLauncher/assets",
      runtimes: "C:/Users/player/AppData/Local/MinLauncher/runtimes",
    },
    install_dir_error: null,
    autostart_supported: autostartSupported,
    jvm_flags: [
      {
        profile: "default",
        decisions: [
          {
            flag: "-XX:+AlwaysPreTouch",
            applied: true,
            effect_ms: 727,
            reason: "Так предписывает версия игры",
          },
        ],
      },
      {
        profile: "measured",
        decisions: [
          {
            flag: "-XX:+AlwaysPreTouch",
            applied: false,
            effect_ms: -727,
            reason: "Замер на этой машине: без этого флага виртуальная машина стартует быстрее",
          },
        ],
      },
    ],
    jvm_collectors: [
      {
        name: "G1",
        flag: "-XX:+UseG1GC",
        start_ms: 311,
        game_rss_mib: 1499,
        reason: "Виртуальная машина стартует дольше, а игра занимает меньше памяти",
      },
      {
        name: "Z",
        flag: "-XX:+UseZGC",
        start_ms: 246,
        game_rss_mib: 1758,
        reason: "Сборщик, который предписывает версия игры: старт и память измерены",
      },
    ],
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
    pickInstallDirectory.mockReset();
    writeSettings.mockResolvedValue(undefined);
    pickInstallDirectory.mockResolvedValue(null);
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

  it("keeps the settings the screen does not show", async () => {
    // The copy of the settings in the frontend is a copy: a field that
    // is missing from it goes back to its default when the player
    // presses the button, and the settings of the mods are lost.
    readSettings.mockResolvedValue(settings());
    renderScreen();

    await screen.findByLabelText("Максимальная куча");
    await userEvent.click(screen.getByRole("button", { name: "Сохранить" }));

    await waitFor(() => expect(writeSettings).toHaveBeenCalledTimes(1));
    const sent = writeSettings.mock.calls[0]?.[0];
    expect(sent?.mods.preset).toBe("balanced");
    expect(sent?.restart.max_restarts).toBe(3);
  });

  it("saves the rules of the restart", async () => {
    readSettings.mockResolvedValue(settings());
    renderScreen();

    await screen.findByLabelText("Максимальная куча");
    await userEvent.click(screen.getByRole("checkbox", { name: /Перезапускать игру при падении/ }));
    await userEvent.click(screen.getByRole("button", { name: "Сохранить" }));

    await waitFor(() => expect(writeSettings).toHaveBeenCalledTimes(1));
    expect(writeSettings.mock.calls[0]?.[0]?.restart.enabled).toBe(false);
  });

  it("hides the autostart where the system cannot do it", async () => {
    // A toggle that does nothing is worse than no toggle at all.
    readSettings.mockResolvedValue(settings({}, false));
    renderScreen();

    await screen.findByLabelText("Максимальная куча");
    expect(
      screen.queryByRole("checkbox", { name: /Запускать вместе с системой/ }),
    ).not.toBeInTheDocument();
  });

  it("shows the autostart where the system can do it", async () => {
    readSettings.mockResolvedValue(settings());
    renderScreen();

    await screen.findByLabelText("Максимальная куча");
    expect(
      screen.getByRole("checkbox", { name: /Запускать вместе с системой/ }),
    ).toBeInTheDocument();
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

  it("shows what the measurement said about the flags of the virtual machine", async () => {
    // The choice of the profile has to say which flags it touches and
    // what the measurement said about them: a flag that is on because
    // of a measurement has to say which measurement.
    readSettings.mockResolvedValue(settings());
    renderScreen();

    const profile = await screen.findByLabelText("Флаги виртуальной машины");
    expect(profile).toHaveValue("default");
    expect(screen.getByText("-XX:+AlwaysPreTouch")).toBeInTheDocument();
    expect(
      screen.getByText(/включён — виртуальная машина стартует на 727 мс дольше/),
    ).toBeInTheDocument();

    await userEvent.selectOptions(profile, "measured");

    expect(
      screen.getByText(/выключен — виртуальная машина стартует на 727 мс быстрее/),
    ).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Сохранить" }));

    await waitFor(() => expect(writeSettings).toHaveBeenCalledTimes(1));
    expect(writeSettings.mock.calls[0]?.[0]?.jvm.flags_profile).toBe("measured");
  });

  it("offers only the collectors the java of mojang has", async () => {
    // The java of mojang does not know Shenandoah: it answers
    // "Unrecognized VM option 'UseShenandoah'" and the game does not
    // start. A choice that cannot start the game must not be offered.
    readSettings.mockResolvedValue(settings());
    renderScreen();

    const collector = (await screen.findByLabelText("Сборщик мусора")) as HTMLSelectElement;
    const values = Array.from(collector.options).map((option) => option.value);

    expect(values).toEqual(["", "G1", "Z", "Parallel"]);
  });

  it("shows what the measurement said about the collectors", async () => {
    // A collector is a flag of the virtual machine, and the rule of the
    // project is the same for it as for every other flag: the choice is
    // offered with the measurement next to it.
    readSettings.mockResolvedValue(settings());
    renderScreen();

    await screen.findByLabelText("Сборщик мусора");

    expect(screen.getByText(/311 ms старта, 1499 МиБ в игре/)).toBeInTheDocument();
    expect(screen.getByText(/246 ms старта, 1758 МиБ в игре/)).toBeInTheDocument();
  });

  it("shows the installation folder and the effective game paths", async () => {
    const view = settings({ install_dir: "E:/MinLauncher" });
    view.directories.install = "E:/MinLauncher";
    view.directories.versions = "E:/MinLauncher/versions";
    view.directories.libraries = "E:/MinLauncher/libraries";
    readSettings.mockResolvedValue(view);
    renderScreen();

    const folder = await screen.findByLabelText("Папка установки");
    expect(folder).toHaveValue("E:/MinLauncher");
    expect(screen.getByText("E:/MinLauncher/versions")).toBeInTheDocument();
    expect(screen.getByText("E:/MinLauncher/libraries")).toBeInTheDocument();
  });

  it("lets the player choose a folder with the native picker", async () => {
    readSettings.mockResolvedValue(settings());
    pickInstallDirectory.mockResolvedValue("E:/Games/Minecraft");
    renderScreen();

    const folder = await screen.findByLabelText("Папка установки");
    await userEvent.click(screen.getByRole("button", { name: "Выбрать…" }));

    await waitFor(() => expect(folder).toHaveValue("E:/Games/Minecraft"));
  });

  it("sends the chosen installation folder on save", async () => {
    readSettings.mockResolvedValue(settings());
    renderScreen();

    const folder = await screen.findByLabelText("Папка установки");
    await userEvent.clear(folder);
    await userEvent.click(folder);
    await userEvent.paste("E:\\MinLauncher");
    await userEvent.click(screen.getByRole("button", { name: "Сохранить" }));

    await waitFor(() => expect(writeSettings).toHaveBeenCalledTimes(1));
    expect(writeSettings.mock.calls[0]?.[0]?.install_dir).toBe("E:\\MinLauncher");
  });

  it("resets the installation folder to the default layout", async () => {
    readSettings.mockResolvedValue(settings({ install_dir: "E:/MinLauncher" }));
    renderScreen();

    const folder = await screen.findByLabelText("Папка установки");
    expect(folder).toHaveValue("E:/MinLauncher");

    await userEvent.click(screen.getByRole("button", { name: "Сбросить" }));
    expect(folder).toHaveValue("");

    await userEvent.click(screen.getByRole("button", { name: "Сохранить" }));
    await waitFor(() => expect(writeSettings).toHaveBeenCalledTimes(1));
    expect(writeSettings.mock.calls[0]?.[0]?.install_dir).toBe("");
  });

  it("shows why the configured folder is not in use", async () => {
    const view = settings({ install_dir: "E:/MinLauncher" });
    view.install_dir_error = "the disk E is not available";
    readSettings.mockResolvedValue(view);
    renderScreen();

    expect(
      await screen.findByText(/Настройка указывает на папку, которую не удалось открыть/),
    ).toBeInTheDocument();
    expect(screen.getByText(/the disk E is not available/)).toBeInTheDocument();
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
