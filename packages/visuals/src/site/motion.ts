export function clamp(value: number, min: number, max: number) {
  return value < min ? min : value > max ? max : value;
}

export function easeOutCubic(value: number) {
  const p = clamp(value, 0, 1);
  return 1 - Math.pow(1 - p, 3);
}

export function damp(
  current: number,
  target: number,
  lambda: number,
  dt: number,
) {
  return current + (target - current) * (1 - Math.exp(-lambda * dt));
}

export function osc(time: number, speed: number, phase: number, amp: number) {
  return Math.sin(time * speed + phase) * amp;
}

export function fract(value: number) {
  return value - Math.floor(value);
}

export function wind(time: number, x: number, z: number) {
  return (
    0.55 +
    0.24 * Math.sin(time * 0.62 + x * 0.31 + z * 0.19) +
    0.12 * Math.sin(time * 1.43 + x * 0.7 - z * 0.4) +
    0.05 * Math.sin(time * 2.9 + z * 0.8)
  );
}

export function hash3(x: number, y: number, z: number) {
  let value =
    Math.imul(x + 374761393, 668265263) ^
    Math.imul(y + 1442695041, 1274126177) ^
    Math.imul(z + 2246822519, 3266489917);
  value = Math.imul(value ^ (value >>> 13), 1274126177);
  return ((value ^ (value >>> 16)) >>> 0) / 0xffffffff;
}
