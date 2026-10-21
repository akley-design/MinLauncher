/**
 * The launches the launcher prepared.
 *
 * The time the launcher spends on a launch is the number the whole
 * project is about: it is the share of the start that belongs to the
 * launcher, and the promise is that it is not more than five seconds.
 *
 * The records live in the memory of the tab and not in the database:
 * writing them on the path of a launch would cost a millisecond that the
 * measurement would see.
 */

/**
 * The time the launcher promises to spend on a launch.
 *
 * The promise is "не более пяти секунд от клика до процесса игры", and
 * the launcher's own share of it is what `prepareMs` measures.
 */
export const LAUNCH_BUDGET_MS = 5000;

/** One launch that the launcher prepared. */
export interface LaunchRecord {
  /** The version that was started. */
  versionId: string;
  /** How long the preparation took, in milliseconds. */
  prepareMs: number;
  /** When the launch happened, in milliseconds since the epoch. */
  at: number;
}

/** The launches of one version, gathered together. */
export interface LaunchSummary {
  /** The version the launches belong to. */
  versionId: string;
  /** How many launches there were. */
  count: number;
  /** The middle of the preparations, in milliseconds. */
  median: number;
  /** The fastest preparation, in milliseconds. */
  fastest: number;
  /** The slowest preparation, in milliseconds. */
  slowest: number;
}

/**
 * Gather the launches of every version together.
 *
 * The versions come out in the order of their last launch. The middle
 * of an even number of preparations is the mean of the two middle ones.
 */
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

/**
 * The middle of a list of numbers that is not empty.
 *
 * The list has to be sorted. With an even number of values the middle
 * is the mean of the two middle ones.
 */
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
