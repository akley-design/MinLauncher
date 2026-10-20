/**
 * The screen of the performance.
 *
 * This screen shows what was **measured**, not what somebody typed in.
 * The numbers come from the same database the benchmark harness writes
 * into, so the table cannot disagree with the harness.
 *
 * The verdict of the comparison is what matters: the number of a single
 * run says nothing, the comparison against the other launcher says
 * whether the launcher got faster.
 */

import { useQuery } from "@tanstack/react-query";
import { useMemo } from "react";

import { EmptyState, ErrorState, LoadingState } from "../components/States";
import { latestBenchmarks } from "../ipc/client";
import { queryKeys } from "../ipc/queries";
import { formatValue } from "../lib/format";

/** The screen of the performance. */
export function PerformanceRoute() {
  const benchmarks = useQuery({
    queryKey: queryKeys.benchmarks(),
    queryFn: latestBenchmarks,
  });

  const records = useMemo(() => benchmarks.data?.records ?? [], [benchmarks.data]);

  if (benchmarks.isPending) {
    return <LoadingState label="Читаем результаты замеров…" />;
  }

  if (benchmarks.isError) {
    return <ErrorState error={benchmarks.error} />;
  }

  return (
    <div className="screen">
      <div className="screen__header">
        <div>
          <h1 className="screen__title">Производительность</h1>
          <p className="screen__subtitle">Результаты измерителя, а не оценка на глаз</p>
        </div>
      </div>

      {records.length === 0 ? (
        <EmptyState
          title="Замеров пока нет"
          description="Запустите измеритель, и его результаты появятся здесь."
        />
      ) : (
        <div className="card" style={{ padding: 0 }}>
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
          Сравнение с эталонным лаунчером ставится по медианам обеих сторон: вердикт не должен
          зависеть от того, сколько замеров попало в серию.
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
