/** Formatting helpers shared by the screens. */

function formatNumber(value: number, digits: number): string {
  return new Intl.NumberFormat("ru-RU", {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
    useGrouping: false,
  }).format(value);
}

/** ISO-8601 from the backend, shown in the reader's time zone. */
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

/** A measurement with the unit the harness recorded. */
export function formatValue(value: number, unit: string): string {
  if (unit === "ms") {
    return value >= 1000 ? `${formatNumber(value / 1000, 2)} с` : `${formatNumber(value, 1)} мс`;
  }
  if (unit === "fps") {
    return formatNumber(value, 1);
  }
  if (unit === "mb") {
    return `${formatNumber(value, 1)} МБ`;
  }
  if (unit === "ratio") {
    return `${formatNumber(value * 100, 1)} %`;
  }
  return `${formatNumber(value, 2)} ${unit}`;
}

/** Improvement in percent; positive is better and always carries a sign. */
export function formatImprovement(percent: number | null): string {
  if (percent === null || !Number.isFinite(percent)) {
    return "—";
  }

  const sign = percent > 0 ? "+" : percent < 0 ? "-" : "";
  return `${sign}${formatNumber(Math.abs(percent), 1)} %`;
}

/** Bytes in the largest unit that keeps the number short. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return "0 Б";
  }

  const units = ["Б", "КБ", "МБ", "ГБ", "ТБ"];
  let value = bytes;
  let unit = 0;

  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }

  const text = formatNumber(value, unit === 0 ? 0 : 1);
  return `${text} ${units[unit]}`;
}
