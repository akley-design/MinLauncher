export async function launcherFixture(page, state = "ready") {
  await page.route("**/functions/v1/launcher-nickname", (route) =>
    route.fulfill({
      json: { claim_token: "design-claim", status: "reserved" },
    }),
  );
  await page.addInitScript(
    ({ state }) => {
      const callbacks = new Map(),
        events = new Map();
      let nextId = 0;
      window.isTauri = true;
      window.__designState = state;
      window.__designCommands = [];
      const config = {
        schema_version: 1,
        language: "ru-RU",
        minimize_to_tray: true,
        autostart: false,
        default_version: null,
        default_jre: "auto",
        show_console: false,
        telemetry_enabled: false,
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
          garbage_collector: "",
          extra_args: [],
          enable_agent: false,
          flags_profile: "default",
        },
        optimization: { profile: "original" },
        mods: { enabled: false, loader_version: "" },
        restart: {
          enabled: false,
          max_restarts: 3,
          backoff_ms: 1000,
          window_ms: 60000,
        },
      };
      window.__designEmit = (event, payload) => {
        for (const [id, entry] of events)
          if (entry.event === event)
            callbacks.get(entry.handler)?.({ event, id, payload });
      };
      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
        unregisterListener: (_event, id) => events.delete(id),
      };
      window.__TAURI_INTERNALS__ = {
        metadata: {
          currentWindow: { label: "main" },
          currentWebview: { label: "main" },
        },
        transformCallback: (callback) => {
          callbacks.set(++nextId, callback);
          return nextId;
        },
        unregisterCallback: (id) => callbacks.delete(id),
        invoke: async (command, args = {}) => {
          window.__designCommands.push({ command, args });
          if (command === "plugin:event|listen") {
            const id = ++nextId;
            events.set(id, args);
            return id;
          }
          if (command === "plugin:event|unlisten") {
            events.delete(args.eventId);
            return;
          }
          if (command === "plugin:window|is_maximized") return false;
          if (command === "plugin:window|is_focused") return true;
          if (command === "plugin:window|is_minimized") return false;
          if (command === "plugin:deep-link|get_current") return [];
          if (command.startsWith("plugin:")) return null;
          if (window.__designState === "error" && command === "list_versions")
            throw {
              code: "network",
              message:
                "Не удалось прочитать список версий. Проверь подключение и попробуй снова.",
            };
          if (command === "list_versions")
            return {
              versions:
                window.__designState === "empty"
                  ? []
                  : [
                      {
                        id: "1.21.4",
                        kind: "release",
                        installed: true,
                        java_major: 21,
                      },
                      {
                        id: "1.20.1",
                        kind: "release",
                        installed: true,
                        java_major: 17,
                      },
                      {
                        id: "1.12.2",
                        kind: "release",
                        installed: false,
                        java_major: 8,
                      },
                      {
                        id: "26.4-snapshot-5",
                        kind: "snapshot",
                        installed: false,
                        java_major: 25,
                      },
                    ],
              latest_release: "1.21.4",
              latest_snapshot: "26.4-snapshot-5",
              installed_count: window.__designState === "empty" ? 0 : 2,
            };
          if (command === "list_accounts")
            return {
              accounts:
                window.__designState === "empty"
                  ? []
                  : [
                      {
                        id: "alex",
                        uuid: "demo",
                        username: "Alex",
                        kind: "offline",
                        selected: true,
                        created_at: "2026-10-25",
                      },
                    ],
              selected_id: window.__designState === "empty" ? null : "alex",
              nickname_claims: [],
            };
          if (command === "list_instances")
            return {
              instances:
                window.__designState === "empty"
                  ? []
                  : [
                      {
                        id: "home",
                        version_id: "1.21.4",
                        name: "Мой мир",
                        game_dir: "C:/MinLauncher/instances/home",
                        java_path: null,
                        jvm_args: null,
                        created_at: "2026-10-25",
                      },
                      {
                        id: "forge",
                        version_id: "1.20.1-forge-47.4.20",
                        name: "Сборка с друзьями",
                        game_dir: "C:/MinLauncher/instances/friends",
                        java_path: null,
                        jvm_args: null,
                        created_at: "2026-10-25",
                      },
                    ],
            };
          if (command === "check_for_update")
            return {
              configured: true,
              available: false,
              current_version: "0.1.0",
              version: null,
              notes: null,
              raw_json: null,
            };
          if (command === "read_settings")
            return {
              config,
              directories: {
                data: "C:/MinLauncher",
                cache: "C:/MinLauncher/cache",
                database: "C:/MinLauncher/state.db",
              },
              autostart_supported: true,
              jvm_flags: [],
              jvm_collectors: [],
            };
          if (command === "write_settings") {
            Object.assign(config, args.config);
            return {
              config,
              directories: {
                data: "C:/MinLauncher",
                cache: "C:/MinLauncher/cache",
                database: "C:/MinLauncher/state.db",
              },
              autostart_supported: true,
              jvm_flags: [],
              jvm_collectors: [],
            };
          }
          if (command === "list_runtimes") return [];
          if (command === "latest_benchmarks") return { records: [] };
          if (command === "compare_in_game") return { comparisons: [] };
          if (command === "list_downloads")
            return window.__designState === "empty"
              ? []
              : [
                  {
                    url: "https://example.invalid/client.jar",
                    sha1: null,
                    size: 32000000,
                    target_path: "versions/1.21.4/client.jar",
                    state: "downloading",
                    bytes_done: 18900000,
                    attempts: 1,
                    error: null,
                    updated_at: "2026-10-25T18:00:00Z",
                    task: "Minecraft 1.21.4",
                  },
                  {
                    url: "https://example.invalid/asset",
                    sha1: null,
                    size: 64000,
                    target_path: "assets/objects/library",
                    state: "failed",
                    bytes_done: 0,
                    attempts: 3,
                    error: "Соединение прервано",
                    updated_at: "2026-10-25T18:00:00Z",
                    task: "Minecraft 1.20.1",
                  },
                ];
          if (command === "launch_game") {
            const game = {
              version_id: args.options.version_id,
              pid: 12345,
              command: "java",
              started_at: 0,
            };
            window.__designEmit("minlauncher://game", {
              kind: "started",
              game,
            });
            return { game, prepare_ms: 24 };
          }
          if (command === "stop_game") {
            window.__designEmit("minlauncher://game", {
              kind: "exited",
              version_id: "1.21.4",
              success: true,
              stopped: true,
            });
            return;
          }
          if (command === "read_manifest") return null;
          return null;
        },
      };
    },
    { state },
  );
}
