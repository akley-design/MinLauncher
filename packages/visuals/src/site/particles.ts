import {
  AdditiveBlending,
  BufferGeometry,
  Color,
  DataTexture,
  Float32BufferAttribute,
  LinearFilter,
  NormalBlending,
  Points,
  RGBAFormat,
  ShaderMaterial,
  UnsignedByteType,
} from "three";
import { hash3 } from "./motion";

export type ParticleMotion =
  "firefly" | "smoke" | "spray" | "dust" | "ember" | "leaf";

export interface ParticleOptions {
  count: number;
  motion: ParticleMotion;
  colors: string[];
  origin: [number, number, number];
  spread: [number, number, number];
  size: number;
  sizeJitter?: number;
  speed?: number;
  opacity?: number;
  additive?: boolean;
}

const MOTION_CODE: Record<ParticleMotion, number> = {
  firefly: 0,
  smoke: 1,
  spray: 2,
  dust: 3,
  ember: 4,
  leaf: 5,
};

const VERTEX = `
uniform float uTime;
uniform float uPixelRatio;
uniform float uMotion;
uniform float uSpeed;
uniform float uOpacity;
attribute float aSeed;
attribute float aSize;
attribute vec3 aColor;
varying vec3 vColor;
varying float vAlpha;

void main() {
  vec3 p = position;
  float life = fract(aSeed + uTime * uSpeed);
  float size = aSize;
  vAlpha = 1.0;

  if (uMotion < 0.5) {
    p.x += sin(uTime * 0.55 + aSeed * 6.283) * 0.22;
    p.y += sin(uTime * 0.85 + aSeed * 9.0) * 0.16;
    p.z += cos(uTime * 0.48 + aSeed * 5.0) * 0.22;
    vAlpha = 0.4 + 0.6 * pow(0.5 + 0.5 * sin(uTime * 1.9 + aSeed * 24.0), 2.0);
  } else if (uMotion < 1.5) {
    p.y += life * 1.0;
    p.x += sin(life * 7.0 + aSeed * 6.283) * 0.16 * life;
    p.z += cos(life * 6.0 + aSeed * 4.0) * 0.12 * life;
    vAlpha = smoothstep(0.0, 0.12, life) * (1.0 - life) * 0.8;
    size *= 1.0 + life * 1.5;
  } else if (uMotion < 2.5) {
    p.y += life * 0.7 - life * life * 1.5;
    p.x += sin(aSeed * 31.0) * life * 0.45;
    p.z += cos(aSeed * 17.0) * life * 0.45;
    vAlpha = (1.0 - life) * 0.75;
  } else if (uMotion < 3.5) {
    p.x += sin(uTime * 0.24 + aSeed * 6.283) * 0.26;
    p.y += sin(uTime * 0.19 + aSeed * 8.0) * 0.2;
    vAlpha = 0.25 + 0.35 * (0.5 + 0.5 * sin(uTime * 0.7 + aSeed * 12.0));
  } else if (uMotion < 4.5) {
    p.y += life * 0.9;
    p.x += sin(life * 7.0 + aSeed * 9.0) * 0.2;
    p.z += cos(life * 6.0 + aSeed * 7.0) * 0.16;
    vAlpha = (1.0 - life) * 0.9;
  } else {
    p.y -= life * 1.6;
    p.x += sin(life * 9.0 + aSeed * 6.283) * 0.24;
    p.z += cos(life * 7.0 + aSeed * 3.0) * 0.22;
    vAlpha = (1.0 - life) * 0.85;
  }

  vColor = aColor;
  vec4 mv = modelViewMatrix * vec4(p, 1.0);
  gl_Position = projectionMatrix * mv;
  gl_PointSize = size * uPixelRatio;
}
`;

const FRAGMENT = `
uniform float uOpacity;
varying vec3 vColor;
varying float vAlpha;

void main() {
  float d = length(gl_PointCoord - vec2(0.5));
  float a = smoothstep(0.5, 0.06, d);
  if (a < 0.01) discard;
  gl_FragColor = vec4(vColor, a * vAlpha * uOpacity);
  #include <tonemapping_fragment>
  #include <colorspace_fragment>
}
`;

export interface ParticleField {
  object: Points;
  material: ShaderMaterial;
  setPixelRatio: (ratio: number) => void;
  dispose: () => void;
}

export function createParticleField(options: ParticleOptions): ParticleField {
  const count = options.count;
  const positions = new Float32Array(count * 3);
  const seeds = new Float32Array(count);
  const sizes = new Float32Array(count);
  const colors = new Float32Array(count * 3);
  const palette = options.colors.map((color) => new Color(color));

  for (let index = 0; index < count; index++) {
    const sx = hash3(index, 1, 17);
    const sy = hash3(index, 2, 29);
    const sz = hash3(index, 3, 43);
    positions[index * 3] = (sx - 0.5) * options.spread[0];
    positions[index * 3 + 1] = (sy - 0.5) * options.spread[1];
    positions[index * 3 + 2] = (sz - 0.5) * options.spread[2];
    seeds[index] = hash3(index, 5, 61);
    sizes[index] =
      options.size * (1 - (options.sizeJitter ?? 0.35) / 2) +
      options.size * (options.sizeJitter ?? 0.35) * hash3(index, 7, 71);
    const color = palette[index % palette.length]!;
    colors[index * 3] = color.r;
    colors[index * 3 + 1] = color.g;
    colors[index * 3 + 2] = color.b;
  }

  const geometry = new BufferGeometry();
  geometry.setAttribute("position", new Float32BufferAttribute(positions, 3));
  geometry.setAttribute("aSeed", new Float32BufferAttribute(seeds, 1));
  geometry.setAttribute("aSize", new Float32BufferAttribute(sizes, 1));
  geometry.setAttribute("aColor", new Float32BufferAttribute(colors, 3));

  const material = new ShaderMaterial({
    uniforms: {
      uTime: { value: 0 },
      uPixelRatio: { value: 1.5 },
      uMotion: { value: MOTION_CODE[options.motion] },
      uSpeed: { value: options.speed ?? 0.1 },
      uOpacity: { value: options.opacity ?? 1 },
    },
    vertexShader: VERTEX,
    fragmentShader: FRAGMENT,
    transparent: true,
    depthWrite: false,
    blending: options.additive ? AdditiveBlending : NormalBlending,
  });

  const object = new Points(geometry, material);
  object.name = `ml-site-particles-${options.motion}`;
  object.position.set(...options.origin);
  object.frustumCulled = false;
  object.renderOrder = 6;

  return {
    object,
    material,
    setPixelRatio(ratio: number) {
      material.uniforms.uPixelRatio!.value = ratio;
    },
    dispose() {
      geometry.dispose();
      material.dispose();
    },
  };
}

export function createGlowTexture() {
  const size = 32;
  const pixels = new Uint8Array(size * size * 4);
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const dx = ((x + 0.5) / size) * 2 - 1;
      const dy = ((y + 0.5) / size) * 2 - 1;
      const distance = Math.min(1, Math.hypot(dx, dy));
      const alpha = Math.pow(1 - distance, 2.2) * 255;
      const offset = (y * size + x) * 4;
      pixels[offset] = 255;
      pixels[offset + 1] = 255;
      pixels[offset + 2] = 255;
      pixels[offset + 3] = alpha;
    }
  }
  const texture = new DataTexture(
    pixels,
    size,
    size,
    RGBAFormat,
    UnsignedByteType,
  );
  texture.name = "ml-site-glow";
  texture.magFilter = LinearFilter;
  texture.minFilter = LinearFilter;
  texture.generateMipmaps = false;
  texture.needsUpdate = true;
  return texture;
}
