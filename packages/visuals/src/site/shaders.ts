import { Color, ShaderMaterial, DoubleSide } from "three";

const VERTEX_UV = `
varying vec2 vUv;
void main() {
  vUv = uv;
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
}
`;

const WATER_FRAGMENT = `
uniform float uTime;
uniform vec3 uColor;
uniform vec3 uHighlight;
uniform float uOpacity;
varying vec2 vUv;

void main() {
  float w1 = sin(vUv.y * 18.0 + vUv.x * 4.0 + uTime * 1.05);
  float w2 = sin(vUv.y * 31.0 - vUv.x * 9.0 - uTime * 1.7);
  float sparkle = smoothstep(
    0.86,
    1.0,
    sin(vUv.x * 31.0 + vUv.y * 23.0 + uTime * 2.4) * 0.5 + 0.5
  );
  vec3 col =
    uColor * (0.92 + 0.06 * w1 + 0.04 * w2) +
    uHighlight * (0.08 + sparkle * 0.24 + 0.05 * w1);
  float alpha = uOpacity * (0.94 + 0.06 * w2);
  gl_FragColor = vec4(col, alpha);
  #include <tonemapping_fragment>
  #include <colorspace_fragment>
}
`;

const WATERFALL_FRAGMENT = `
uniform float uTime;
uniform vec3 uColor;
uniform vec3 uHighlight;
uniform float uOpacity;
varying vec2 vUv;

void main() {
  float columns = sin(vUv.x * 42.0 + sin(vUv.y * 7.0 - uTime * 3.0) * 2.2);
  float streak = smoothstep(
    0.2,
    1.0,
    sin(vUv.y * 27.0 - uTime * 7.2 + vUv.x * 9.0) * 0.5 + 0.5
  );
  float edge =
    smoothstep(0.0, 0.08, vUv.x) *
    smoothstep(1.0, 0.92, vUv.x) *
    smoothstep(0.0, 0.1, vUv.y) *
    smoothstep(1.0, 0.9, vUv.y);
  vec3 col =
    uColor * (0.9 + 0.1 * columns) +
    uHighlight * (streak * 0.36 + 0.08);
  float alpha = uOpacity * (0.62 + 0.26 * streak) * edge;
  gl_FragColor = vec4(col, alpha);
  #include <tonemapping_fragment>
  #include <colorspace_fragment>
}
`;

const PORTAL_VERTEX = `
uniform float uTime;
varying vec2 vUv;

void main() {
  vUv = uv;
  vec3 p = position;
  p.z += sin(p.y * 4.0 + uTime * 1.5 + p.x * 2.0) * 0.022;
  gl_Position = projectionMatrix * modelViewMatrix * vec4(p, 1.0);
}
`;

const PORTAL_FRAGMENT = `
uniform float uTime;
uniform float uPulse;
varying vec2 vUv;

float hash(vec2 p) {
  return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123);
}

float noise(vec2 p) {
  vec2 i = floor(p);
  vec2 f = fract(p);
  vec2 u = f * f * (3.0 - 2.0 * f);
  return mix(
    mix(hash(i), hash(i + vec2(1.0, 0.0)), u.x),
    mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), u.x),
    u.y
  );
}

float fbm(vec2 p) {
  float value = 0.0;
  float amplitude = 0.55;
  for (int i = 0; i < 4; i++) {
    value += amplitude * noise(p);
    p = p * 2.02 + vec2(13.7, 7.3);
    amplitude *= 0.5;
  }
  return value;
}

void main() {
  vec2 p = (vUv - 0.5) * vec2(1.6, 2.2);
  float t = uTime * 0.32;
  vec2 q = vec2(
    fbm(p * 2.1 + vec2(t, -t * 0.6)),
    fbm(p * 2.1 + vec2(-t * 0.8, t))
  );
  float swirl = fbm(p * 3.2 + q * 1.8 + vec2(t * 1.2, -t * 0.7));
  float band = smoothstep(0.28, 0.82, swirl + 0.16 * sin(vUv.y * 9.0 + uTime));
  float edge =
    smoothstep(0.0, 0.14, vUv.x) *
    smoothstep(1.0, 0.86, vUv.x) *
    smoothstep(0.0, 0.12, vUv.y) *
    smoothstep(1.0, 0.88, vUv.y);
  vec3 deep = vec3(0.09, 0.03, 0.2);
  vec3 mid = vec3(0.48, 0.12, 0.92);
  vec3 hot = vec3(0.82, 0.54, 1.0);
  vec3 col = mix(deep, mid, band);
  col = mix(col, hot, smoothstep(0.74, 1.0, band) * 0.45);
  col += hot * (1.0 - edge) * 0.34;
  float alpha = (0.7 + 0.22 * edge) * uPulse;
  gl_FragColor = vec4(col * (0.82 + 0.34 * edge), alpha);
  #include <tonemapping_fragment>
  #include <colorspace_fragment>
}
`;

export function createWaterMaterial(
  color: string,
  highlight: string,
  opacity = 0.72,
) {
  return new ShaderMaterial({
    uniforms: {
      uTime: { value: 0 },
      uColor: { value: new Color(color) },
      uHighlight: { value: new Color(highlight) },
      uOpacity: { value: opacity },
    },
    vertexShader: VERTEX_UV,
    fragmentShader: WATER_FRAGMENT,
    transparent: true,
    depthWrite: false,
    side: DoubleSide,
  });
}

export function createWaterfallMaterial(
  color: string,
  highlight: string,
  opacity = 0.66,
) {
  return new ShaderMaterial({
    uniforms: {
      uTime: { value: 0 },
      uColor: { value: new Color(color) },
      uHighlight: { value: new Color(highlight) },
      uOpacity: { value: opacity },
    },
    vertexShader: VERTEX_UV,
    fragmentShader: WATERFALL_FRAGMENT,
    transparent: true,
    depthWrite: false,
    side: DoubleSide,
  });
}

export function createPortalMaterial() {
  return new ShaderMaterial({
    uniforms: {
      uTime: { value: 0 },
      uPulse: { value: 1 },
    },
    vertexShader: PORTAL_VERTEX,
    fragmentShader: PORTAL_FRAGMENT,
    transparent: true,
    depthWrite: false,
    side: DoubleSide,
  });
}
