/**
 * The screen of the settings.
 *
 * The settings are read whole and written whole. The screen keeps a
 * copy of what was read, edits the copy, and writes it back when the
 * player presses the button — so that a half-filled form never reaches
 * the disk, and so that the form can be closed without a change.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";

import { Button } from "../components/Button";
import { JavaRuntimes } from "../components/JavaRuntimes";
import { ErrorState, LoadingState } from "../components/States";
import { Updates } from "../components/Updates";
import { readSettings, toCommandError, writeSettings } from "../ipc/client";
import { queryKeys } from "../ipc/queries";
import type {
  CollectorMeasurement,
  Directories,
  FlagsProfile,
  LauncherConfig,
  ProfileFlags,
} from "../ipc/types";

/** The screen of the settings. */
export function SettingsRoute() {
  const queryClient = useQueryClient();

  const settings = useQuery({
    queryKey: queryKeys.settings(),
    queryFn: readSettings,
  });

  const save = useMutation({
    mutationFn: writeSettings,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.settings() });
    },
  });

  if (settings.isPending) {
    return <LoadingState label="Читаем настройки…" />;
  }

  if (settings.isError) {
    return <ErrorState error={settings.error} />;
  }

  return (
    <div className="screen">
      <div className="screen__header">
        <div>
          <h1 className="screen__title">Настройки</h1>
          <p className="screen__subtitle">Сохраняются в файле рядом с базой</p>
        </div>
      </div>

      {save.isError ? <p className="field__error">{toCommandError(save.error).message}</p> : null}
      {save.isSuccess ? <p className="field__hint">Сохранено.</p> : null}

      {/*
       * The form is a child of the screen, and it is keyed on the
       * settings it was given. When the backend sends something new —
       * after a save, or after the player changed the file by hand —
       * the key changes, and React throws the old form away and builds
       * a new one from the new settings. The copy the player edits is
       * therefore never out of date, and no effect has to keep the two
       * in sync.
       */}
      <SettingsForm
        key={JSON.stringify(settings.data.config)}
        config={settings.data.config}
        directories={settings.data.directories}
        autostartSupported={settings.data.autostart_supported}
        jvmFlags={settings.data.jvm_flags}
        jvmCollectors={settings.data.jvm_collectors ?? []}
        busy={save.isPending}
        onSave={(config) => save.mutate(config)}
      />
    </div>
  );
}

interface SettingsFormProps {
  /** The settings as they are now. */
  config: LauncherConfig;
  /** The directories the launcher uses. */
  directories: Directories;
  /** Whether this system can start the launcher with itself. */
  autostartSupported: boolean;
  /** What the measurement said about the flags of the virtual machine. */
  jvmFlags: ProfileFlags[];
  /** What the measurement said about the collectors of the garbage. */
  jvmCollectors: CollectorMeasurement[];
  /** Whether a save is under way. */
  busy: boolean;
  /** The player asked to save the copy. */
  onSave: (config: LauncherConfig) => void;
}

/** The editable copy of the settings. */
function SettingsForm({
  config,
  directories,
  autostartSupported,
  jvmFlags,
  jvmCollectors,
  busy,
  onSave,
}: SettingsFormProps) {
  const [draft, setDraft] = useState<LauncherConfig>(config);

  const patch = (change: Partial<LauncherConfig>) => {
    setDraft({ ...draft, ...change });
  };

  // The list under the choice follows the choice: the player sees what
  // the profile does before saving it.
  const decisions =
    jvmFlags.find((entry) => entry.profile === draft.jvm.flags_profile)?.decisions ?? [];

  return (
    <>
      <section className="card">
        <div className="card__title">Запуск</div>

        <div className="settings-grid">
          <label className="toggle">
            <input
              type="checkbox"
              checked={draft.minimize_to_tray}
              onChange={(event) => patch({ minimize_to_tray: event.target.checked })}
            />
            <span>
              <span className="toggle__title">Сворачивать в трей</span>
              <span className="toggle__hint">
                Закрытие окна прячет лаунчер, а игра продолжает идти
              </span>
            </span>
          </label>

          {autostartSupported ? (
            <label className="toggle">
              <input
                type="checkbox"
                checked={draft.autostart}
                onChange={(event) => patch({ autostart: event.target.checked })}
              />
              <span>
                <span className="toggle__title">Запускать вместе с системой</span>
                <span className="toggle__hint">Лаунчер открывается при входе в Windows</span>
              </span>
            </label>
          ) : null}

          <label className="toggle">
            <input
              type="checkbox"
              checked={draft.restart.enabled}
              onChange={(event) =>
                patch({ restart: { ...draft.restart, enabled: event.target.checked } })
              }
            />
            <span>
              <span className="toggle__title">Перезапускать игру при падении</span>
              <span className="toggle__hint">
                Игра, которая упала сама, запускается заново. Закрытая игроком — нет
              </span>
            </span>
          </label>

          <label className="toggle">
            <input
              type="checkbox"
              checked={draft.show_console}
              onChange={(event) => patch({ show_console: event.target.checked })}
            />
            <span>
              <span className="toggle__title">Показывать консоль</span>
              <span className="toggle__hint">Окно лаунчера с выводом, полезно при отладке</span>
            </span>
          </label>

          <label className="toggle">
            <input
              type="checkbox"
              checked={draft.telemetry_enabled}
              onChange={(event) => patch({ telemetry_enabled: event.target.checked })}
            />
            <span>
              <span className="toggle__title">Собирать замеры производительности</span>
              <span className="toggle__hint">
                Обезличенные замеры помогают понять, что стало медленнее
              </span>
            </span>
          </label>
        </div>

        <div className="form-row" style={{ marginTop: "var(--space-4)" }}>
          <div className="field" style={{ flex: 1 }}>
            <label className="field__label" htmlFor="restarts">
              Перезапусков подряд
            </label>
            <input
              id="restarts"
              className="input"
              type="number"
              min={0}
              max={20}
              value={draft.restart.max_restarts}
              onChange={(event) =>
                patch({
                  restart: {
                    ...draft.restart,
                    max_restarts: Number(event.target.value) || 0,
                  },
                })
              }
            />
            <span className="field__hint">Сколько раз игра поднимется сама</span>
          </div>

          <div className="field" style={{ flex: 1 }}>
            <label className="field__label" htmlFor="language">
              Язык интерфейса
            </label>
            <input
              id="language"
              className="input"
              value={draft.language}
              onChange={(event) => patch({ language: event.target.value })}
              placeholder="ru-RU"
            />
          </div>

          <div className="field" style={{ flex: 1 }}>
            <label className="field__label" htmlFor="default-jre">
              Java по умолчанию
            </label>
            <input
              id="default-jre"
              className="input"
              value={draft.default_jre}
              onChange={(event) => patch({ default_jre: event.target.value })}
              placeholder="auto"
            />
            <span className="field__hint">«auto» — взять ту версию, что требует игра</span>
          </div>
        </div>
      </section>

      <section className="card">
        <div className="card__title">Память и сборщик мусора</div>

        <div className="form-row">
          <div className="field" style={{ flex: 1 }}>
            <label className="field__label" htmlFor="max-heap">
              Максимальная куча
            </label>
            <input
              id="max-heap"
              className="input"
              value={draft.jvm.max_heap}
              onChange={(event) => patch({ jvm: { ...draft.jvm, max_heap: event.target.value } })}
              placeholder="4G"
            />
            <span className="field__hint">Пусто — определить по памяти машины</span>
          </div>

          <div className="field" style={{ flex: 1 }}>
            <label className="field__label" htmlFor="min-heap">
              Минимальная куча
            </label>
            <input
              id="min-heap"
              className="input"
              value={draft.jvm.min_heap}
              onChange={(event) => patch({ jvm: { ...draft.jvm, min_heap: event.target.value } })}
              placeholder="1G"
            />
          </div>

          <div className="field" style={{ flex: 1 }}>
            <label className="field__label" htmlFor="gc">
              Сборщик мусора
            </label>
            <select
              id="gc"
              className="input"
              value={draft.jvm.garbage_collector}
              onChange={(event) =>
                patch({ jvm: { ...draft.jvm, garbage_collector: event.target.value } })
              }
            >
              <option value="">Как в версии игры</option>
              <option value="G1">G1</option>
              <option value="Z">Z</option>
              <option value="Parallel">Parallel</option>
            </select>
            {/*
             * A collector is a flag of the virtual machine, and the rule
             * of the project is the same for it as for every other flag:
             * the choice is offered with the measurement next to it. The
             * numbers are the ones of this machine — the start of the
             * virtual machine and the memory of the game.
             */}
            {jvmCollectors.map((collector) => (
              <div key={collector.name} className="field__hint">
                <span className="mono">{collector.name}</span> — {collector.reason}:{" "}
                {collector.start_ms} ms старта,{" "}
                {collector.game_rss_mib > 0
                  ? `${collector.game_rss_mib} МиБ в игре`
                  : "в игре не измерялся"}
              </div>
            ))}
          </div>
        </div>

        <label className="toggle" style={{ marginTop: "var(--space-4)" }}>
          <input
            type="checkbox"
            checked={draft.jvm.enable_agent}
            onChange={(event) =>
              patch({ jvm: { ...draft.jvm, enable_agent: event.target.checked } })
            }
          />
          <span>
            <span className="toggle__title">Включить агент профилирования</span>
            <span className="toggle__hint">
              Java-агент замеряет фазы старта игры и пишет их в журнал лаунчера
            </span>
          </span>
        </label>

        <div className="field" style={{ marginTop: "var(--space-4)" }}>
          <label className="field__label" htmlFor="extra-args">
            Дополнительные аргументы JVM
          </label>
          <textarea
            id="extra-args"
            className="input"
            style={{ height: 84, paddingTop: "var(--space-2)", resize: "vertical" }}
            value={draft.jvm.extra_args.join("\n")}
            onChange={(event) =>
              patch({
                jvm: {
                  ...draft.jvm,
                  extra_args: event.target.value
                    .split("\n")
                    .map((line) => line.trim())
                    .filter(Boolean),
                },
              })
            }
            placeholder={"-XX:+UseStringDeduplication\n-XX:+UseG1GC"}
          />
          <span className="field__hint">По одному аргументу в строке</span>
        </div>

        {/*
         * The profile of the flags is the only place where the launcher
         * is allowed to differ from the version of the game — and it
         * differs only where a measurement on this machine showed a
         * win. The list under the choice says which flags that is, what
         * the measurement said, and why the decision was made, so that
         * the choice is never a black box.
         */}
        <div className="field" style={{ marginTop: "var(--space-4)" }}>
          <label className="field__label" htmlFor="flags-profile">
            Флаги виртуальной машины
          </label>
          <select
            id="flags-profile"
            className="input"
            value={draft.jvm.flags_profile}
            onChange={(event) =>
              patch({
                jvm: { ...draft.jvm, flags_profile: event.target.value as FlagsProfile },
              })
            }
          >
            <option value="default">Как в версии игры</option>
            <option value="measured">По замеру на этой машине</option>
          </select>
          <span className="field__hint">
            {draft.jvm.flags_profile === "measured"
              ? "Лаунчер выключает флаги, которые на этой машине замедляли старт"
              : "Лаунчер не трогает флаги, которые предписывает версия игры"}
          </span>

          {decisions.map((decision) => (
            <div key={decision.flag} style={{ marginTop: "var(--space-3)" }}>
              <span className="mono">{decision.flag}</span>
              <span className="field__hint">
                {describe(decision.applied, decision.effect_ms)} — {decision.reason}
              </span>
            </div>
          ))}
        </div>
      </section>

      <section className="card">
        <div className="card__title">Загрузка</div>

        <div className="form-row">
          <div className="field" style={{ flex: 1 }}>
            <label className="field__label" htmlFor="concurrent">
              Одновременных соединений
            </label>
            <input
              id="concurrent"
              className="input"
              type="number"
              min={1}
              max={64}
              value={draft.downloads.max_concurrent}
              onChange={(event) =>
                patch({
                  downloads: {
                    ...draft.downloads,
                    max_concurrent: Number(event.target.value) || 1,
                  },
                })
              }
            />
          </div>

          <div className="field" style={{ flex: 1 }}>
            <label className="field__label" htmlFor="retries">
              Повторов при ошибке
            </label>
            <input
              id="retries"
              className="input"
              type="number"
              min={0}
              max={10}
              value={draft.downloads.max_retries}
              onChange={(event) =>
                patch({
                  downloads: {
                    ...draft.downloads,
                    max_retries: Number(event.target.value) || 0,
                  },
                })
              }
            />
          </div>

          <div className="field" style={{ flex: 1 }}>
            <label className="field__label" htmlFor="timeout">
              Таймаут, с
            </label>
            <input
              id="timeout"
              className="input"
              type="number"
              min={1}
              max={600}
              value={draft.downloads.timeout_secs}
              onChange={(event) =>
                patch({
                  downloads: {
                    ...draft.downloads,
                    timeout_secs: Number(event.target.value) || 1,
                  },
                })
              }
            />
          </div>
        </div>

        <label className="toggle" style={{ marginTop: "var(--space-4)" }}>
          <input
            type="checkbox"
            checked={draft.downloads.verify_checksums}
            onChange={(event) =>
              patch({
                downloads: {
                  ...draft.downloads,
                  verify_checksums: event.target.checked,
                },
              })
            }
          />
          <span>
            <span className="toggle__title">Проверять контрольные суммы</span>
            <span className="toggle__hint">SHA-1 каждого файла сверяется с манифестом Mojang</span>
          </span>
        </label>
      </section>

      <JavaRuntimes />

      <Updates />

      <section className="card">
        <div className="card__title">Папки</div>
        <dl className="paths">
          <div className="paths__row">
            <dt>Данные</dt>
            <dd className="mono selectable">{directories.data}</dd>
          </div>
          <div className="paths__row">
            <dt>Кэш</dt>
            <dd className="mono selectable">{directories.cache}</dd>
          </div>
          <div className="paths__row">
            <dt>База</dt>
            <dd className="mono selectable">{directories.database}</dd>
          </div>
        </dl>
      </section>

      <div className="screen__actions" style={{ justifyContent: "flex-end" }}>
        <Button variant="primary" busy={busy} onClick={() => onSave(draft)}>
          Сохранить
        </Button>
      </div>
    </>
  );
}

/**
 * Say what a decision means, in one line.
 *
 * A positive effect is what the flag costs; a negative one is what it
 * saves. The word is the same in both cases — the sign is what differs.
 *
 * The time is the time of the start of the virtual machine, and not the
 * time of the launch of the game: the flag is read by the virtual
 * machine when it starts, and the game begins long after that. Saying
 * it otherwise would promise a launch that is faster than it is.
 */
function describe(applied: boolean, effectMs: number): string {
  const millis = Math.abs(effectMs);
  const verb = applied ? "включён" : "выключен";
  const cost = applied ? "дольше" : "быстрее";
  return `${verb} — виртуальная машина стартует на ${millis} мс ${cost}`;
}
