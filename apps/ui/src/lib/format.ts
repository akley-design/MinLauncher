/**
 * The way a number is written for a person to read.
 *
 * The helpers here are pure — they take a value and return a string —
 * and they live apart from the screens, so that the screens can hold
 * nothing but components.
 */

/**
 * The time of a release, in the format a player reads.
 *
 * The value that comes from the backend is ISO-8601 in UTC. It is
 * turned into the local time of the machine, because a release date is
 * only meaningful in the time zone of the reader.
 */
export function formatTime(value: string | undefined): string {
  if (!value) {
    return "—";
  }

  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value;
  }

  return parsed.toLocaleDateString("ru-RU", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  });
}

/**
 * A number with its unit.
 *
 * The unit of a measurement is the one the harness recorded: `ms`, `fps`,
 * `mb` or `ratio`. The number is shortened, because a table of numbers
 * with twelve digits in each of them cannot be read at a glance.
 */
export function formatValue(value: number, unit: string): string {
  if (unit === "ms") {
    return value >= 1000 ? `${(value / 1000).toFixed(2)} с` : `${value.toFixed(1)} мс`;
  }
  if (unit === "fps") {
    return value.toFixed(1);
  }
  if (unit === "mb") {
    return `${value.toFixed(1)} МБ`;
  }
  if (unit === "ratio") {
    return `${(value * 100).toFixed(1)} %`;
  }
  return `${value.toFixed(2)} ${unit}`;
}
