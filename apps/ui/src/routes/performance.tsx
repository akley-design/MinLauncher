/** Measurements from the benchmark database, not hand-entered numbers. */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo, useRef, useState } from "react";

import { Button } from "../components/Button";
import { EmptyState, ErrorState, LoadingState } from "../components/States";
import { compareInGame, importTLauncherBaseline, latestBenchmarks } from "../ipc/client";
import { queryKeys } from "../ipc/queries";
import { formatImprovement, formatValue } from "../lib/format";
import { LAUNCH_BUDGET_MS, summarizeLaunches } from "../lib/launches";
import { useGameStore } from "../stores/game";

export function PerformanceRoute() {
  const queryClient = useQueryClient();
  const baselineFile = useRef<HTMLInputElement>(null);
  const [fileError, setFileError] = useState<string | null>(null);
  const baselineImport = useMutation({
    mutationFn: importTLauncherBaseline,
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.benchmarks() }),
        queryClient.invalidateQueries({ queryKey: queryKeys.inGameComparison() }),
      ]);
    },
  });
  const benchmarks = useQuery({
    queryKey: queryKeys.benchmarks(),
    queryFn: latestBenchmarks,
  });

  const inGame = useQuery({
    queryKey: queryKeys.inGameComparison(),
    queryFn: compareInGame,
  });

  // The launches of this session. They are not in the database: the
  // number of the preparation is written on the path of the launch, and
  // a write there would cost the milliseconds it measures.
  const launches = useGameStore((state) => state.launches);

  const records = useMemo(() => benchmarks.data?.records ?? [], [benchmarks.data]);
  const comparisons = useMemo(() => inGame.data?.comparisons ?? [], [inGame.data]);
  const summary = useMemo(() => summarizeLaunches(launches), [launches]);

  if (benchmarks.isPending) {
    return <LoadingState label="Читаем результаты замеров…" />;
  }

  if (benchmarks.isError) {
    return <ErrorState error={benchmarks.error} />;
  }

  return (
    <div className="screen performance-screen">
      <div className="screen__header">
        <div>
          <div className="screen__eyebrow">Аналитика и тесты</div>
          <h1 className="screen__title">Производительность</h1>
          <p className="screen__subtitle">Здесь показаны результаты замеров.</p>
        </div>
      </div>

      <section className="card">
        <div className="card__title">Доля лаунчера в запуске</div>
        {summary.length === 0 ? (
          <p className="field__hint">
            Появится после первого запуска: здесь видно, сколько времени лаунчер потратил на
            подготовку каждой версии.
          </p>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Версия</th>
                <th>Запусков</th>
                <th>Медиана</th>
                <th>Минимум</th>
                <th>Максимум</th>
              </tr>
            </thead>
            <tbody>
              {summary.map((row) => (
                <tr key={row.versionId}>
                  <td className="mono">{row.versionId}</td>
                  <td>{row.count}</td>
                  <td className="mono">
                    {row.median} мс{" "}
                    {row.median > LAUNCH_BUDGET_MS ? (
                      <span className="badge badge--warning">больше 5 с</span>
                    ) : (
                      <span className="badge badge--success">в пределах 5 с</span>
                    )}
                  </td>
                  <td className="mono">{row.fastest} мс</td>
                  <td className="mono">{row.slowest} мс</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      <section className="card">
        <div className="card__title">Сравнение MinLauncher с TLauncher</div>
        <p className="field__hint">
          Сначала соберите 30 игровых запусков TLauncher скриптом из папки проекта, затем выберите
          созданный JSON. Он должен относиться к Minecraft 1.20.1 на этом компьютере.
        </p>
        <p className="field__hint">
          Профиль «Максимум кадров» ставит дальность прорисовки и симуляции на 4 чанка. Результат
          отражает этот обмен качества картинки на скорость.
        </p>
        <input
          ref={baselineFile}
          type="file"
          accept=".json,application/json"
          className="visually-hidden"
          aria-label="Файл замеров TLauncher"
          onChange={(event) => {
            const file = event.currentTarget.files?.[0];
            event.currentTarget.value = "";
            setFileError(null);
            if (!file) return;
            void file
              .text()
              .then((json) => baselineImport.mutate(json))
              .catch((error: unknown) =>
                setFileError(error instanceof Error ? error.message : String(error)),
              );
          }}
        />
        <div className="form-row" style={{ marginTop: "var(--space-3)" }}>
          <Button
            variant="primary"
            busy={baselineImport.isPending}
            onClick={() => baselineFile.current?.click()}
          >
            Импортировать замеры TLauncher
          </Button>
          {baselineImport.data ? (
            <span className="badge badge--success">
              Эталон загружен · {baselineImport.data.runs_per_metric} запусков
            </span>
          ) : null}
        </div>
        {fileError ? (
          <p role="alert" aria-live="polite" className="field__error">
            {fileError}
          </p>
        ) : null}
        {baselineImport.isError ? (
          <p role="alert" aria-live="polite" className="field__error">
            {baselineImport.error.message}
          </p>
        ) : null}

        {comparisons.length === 0 ? (
          <p className="field__hint">
            Сравнение появится после импорта эталона и 30 замеров игры через MinLauncher с профилем
            «Максимум кадров».
          </p>
        ) : (
          <div className="comparison-list">
            {comparisons.map((comparison) => {
              const scale = Math.max(comparison.reference, comparison.optimized, 1);
              const verdictLabel =
                comparison.verdict === "met"
                  ? "Цель достигнута"
                  : comparison.verdict === "missed"
                    ? "Цель не достигнута"
                    : "Недостаточно данных";
              const verdictClass =
                comparison.verdict === "met"
                  ? "badge badge--success"
                  : comparison.verdict === "missed"
                    ? "badge badge--warning"
                    : "badge";

              return (
                <article className="comparison-card" key={comparison.metric}>
                  <div className="comparison-card__heading">
                    <div>
                      <div className="card__eyebrow">
                        {comparison.metric === "in_game_fps" ? "M5 · СКОРОСТЬ" : "M6 · ПЛАВНОСТЬ"}
                      </div>
                      <h3 className="comparison-card__title">
                        {METRIC_LABELS[comparison.metric] ?? comparison.metric}
                      </h3>
                    </div>
                    <span className={verdictClass}>{verdictLabel}</span>
                  </div>
                  <div className="benchmark-compare">
                    <div className="benchmark-stat">
                      <span className="benchmark-stat__label">TLauncher</span>
                      <strong className="benchmark-stat__value">
                        {formatValue(comparison.reference, comparison.unit)}
                      </strong>
                      <span className="benchmark-stat__detail">
                        {comparison.reference_samples} замеров
                      </span>
                    </div>
                    <div className="benchmark-stat benchmark-stat--optimized">
                      <span className="benchmark-stat__label">MinLauncher</span>
                      <strong className="benchmark-stat__value">
                        {formatValue(comparison.optimized, comparison.unit)}
                      </strong>
                      <span className="benchmark-stat__detail">
                        {comparison.optimized_samples} замеров · Max FPS
                      </span>
                    </div>
                    <div className="benchmark-stat benchmark-stat--result">
                      <span className="benchmark-stat__label">Изменение</span>
                      <strong className="benchmark-stat__value">
                        {formatImprovement(comparison.improvement_percent)}
                      </strong>
                      <span className="benchmark-stat__detail">по медиане серии</span>
                    </div>
                  </div>
                  <div className="benchmark-bars" aria-label="Сравнение измеренных значений">
                    <div className="benchmark-bars__row">
                      <span className="benchmark-bars__label">TLauncher</span>
                      <div className="benchmark-bars__track" aria-hidden="true">
                        <div
                          className="benchmark-bars__fill benchmark-bars__fill--reference"
                          style={{ width: `${(comparison.reference / scale) * 100}%` }}
                        />
                      </div>
                      <span className="benchmark-bars__value">
                        {formatValue(comparison.reference, comparison.unit)}
                      </span>
                    </div>
                    <div className="benchmark-bars__row">
                      <span className="benchmark-bars__label">MinLauncher</span>
                      <div className="benchmark-bars__track" aria-hidden="true">
                        <div
                          className="benchmark-bars__fill benchmark-bars__fill--optimized"
                          style={{ width: `${(comparison.optimized / scale) * 100}%` }}
                        />
                      </div>
                      <span className="benchmark-bars__value">
                        {formatValue(comparison.optimized, comparison.unit)}
                      </span>
                    </div>
                  </div>
                </article>
              );
            })}
          </div>
        )}
      </section>

      {records.length === 0 ? (
        <EmptyState
          title={comparisons.length > 0 ? "Замеров запуска лаунчера пока нет" : "Замеров пока нет"}
          description={
            comparisons.length > 0
              ? "Игровое сравнение уже отображается выше. Метрики запуска лаунчера появятся после работы измерителя."
              : "Запустите измеритель, и его результаты появятся здесь."
          }
        />
      ) : (
        <div className="card table-panel">
          <table className="table">
            <thead>
              <tr>
                <th>Метрика</th>
                <th>Сценарий</th>
                <th>Медиана</th>
                <th>99 %</th>
                <th>Замеров</th>
              </tr>
            </thead>
            <tbody>
              {records.map((record) => (
                <tr key={`${record.metric}-${record.scenario}`}>
                  <td>{METRIC_LABELS[record.metric] ?? record.metric}</td>
                  <td className="mono">{record.scenario}</td>
                  <td className="mono">{formatValue(record.p50, record.unit)}</td>
                  <td className="mono">{formatValue(record.p99, record.unit)}</td>
                  <td>{record.samples}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <section className="card">
        <div className="card__title">Что значат эти числа</div>
        <p className="field__hint">
          Медиана — это обычное время, а 99-й перцентиль — худший из сотни замеров. Именно на нём
          видно, что игра подтормаживает.
        </p>
        <p className="field__hint" style={{ marginTop: "var(--space-2)" }}>
          Сравнение с TLauncher считается по медианам обеих сторон: вердикт не должен зависеть от
          того, сколько замеров попало в серию.
        </p>
      </section>
    </div>
  );
}

/** The names of the metrics, in Russian. */
const METRIC_LABELS: Record<string, string> = {
  launcher_cold_start: "Холодный старт лаунчера",
  launcher_warm_start: "Тёплый старт лаунчера",
  click_to_window: "Клик до окна игры",
  fresh_install: "Установка с нуля",
  in_game_fps: "FPS в игре",
  in_game_frametime: "Время кадра",
  launcher_rss: "Память лаунчера",
  game_rss: "Память игры",
};
