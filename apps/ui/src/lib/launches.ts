/**
 * The launches the launcher prepared.
 *
 * The records live in the tab, not in the database: a write on the
 * launch path would cost a millisecond the measurement can see.
 */

/** The promise is five seconds from click to the game process. */
export const LAUNCH_BUDGET_MS = 5000;

export interface LaunchRecord {
  versionId: string;
  prepareMs: number;
  at: number;
}

export interface LaunchSummary {
  versionId: string;
  count: number;
  median: number;
  fastest: number;
  slowest: number;
}

/** Group launches by version. Versions come out in first-seen order. */
export function summarizeLaunches(launches: LaunchRecord[]): LaunchSummary[] {
  const groups = new Map<string, number[]>();

  for (const launch of launches) {
    const times = groups.get(launch.versionId);
    if (times) {
      times.push(launch.prepareMs);
    } else {
      groups.set(launch.versionId, [launch.prepareMs]);
    }
  }

  return [...groups].map(([versionId, times]) => {
    const sorted = [...times].sort((a, b) => a - b);
    return {
      versionId,
      count: sorted.length,
      median: medianOf(sorted),
      fastest: Math.min(...sorted),
      slowest: Math.max(...sorted),
    };
  });
}

/** Middle of a sorted list; even length uses the mean of the two middle values. */
function medianOf(sorted: number[]): number {
  const middle = Math.floor(sorted.length / 2);
  const upper = sorted[middle];

  if (upper === undefined) {
    // The list is empty, and there is no middle to speak of.
    return 0;
  }
  if (sorted.length % 2 === 1) {
    return upper;
  }
  const lower = sorted[middle - 1] ?? upper;
  return Math.round((lower + upper) / 2);
}
