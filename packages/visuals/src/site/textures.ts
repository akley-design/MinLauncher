import {
  ClampToEdgeWrapping,
  DataTexture,
  NearestFilter,
  NearestMipmapLinearFilter,
  RGBAFormat,
  SRGBColorSpace,
  UnsignedByteType,
} from "three";

export type SiteTile =
  | "grass_top"
  | "grass_side"
  | "dirt"
  | "stone"
  | "cobble"
  | "gravel"
  | "sand"
  | "planks"
  | "log_side"
  | "log_top"
  | "birch_side"
  | "birch_top"
  | "plaster"
  | "roof"
  | "leaves"
  | "leaves_birch"
  | "leaves_pine"
  | "obsidian"
  | "deepslate"
  | "ore_copper"
  | "ore_amethyst"
  | "crystal"
  | "lantern"
  | "window"
  | "metal";

export const SITE_TILES: SiteTile[] = [
  "grass_top",
  "grass_side",
  "dirt",
  "stone",
  "cobble",
  "gravel",
  "sand",
  "planks",
  "log_side",
  "log_top",
  "birch_side",
  "birch_top",
  "plaster",
  "roof",
  "leaves",
  "leaves_birch",
  "leaves_pine",
  "obsidian",
  "deepslate",
  "ore_copper",
  "ore_amethyst",
  "crystal",
  "lantern",
  "window",
  "metal",
];

const SIZE = 16;

function hash(x: number, y: number, seed: number) {
  let value =
    Math.imul(x + 374761393, 668265263) ^
    Math.imul(y + 1442695041, 1274126177) ^
    Math.imul(seed + 1, 2654435761);
  value = Math.imul(value ^ (value >>> 13), 1274126177);
  return ((value ^ (value >>> 16)) >>> 0) / 0xffffffff;
}

type Rgb = [number, number, number];

function byte(value: number) {
  return Math.max(0, Math.min(255, Math.round(value)));
}

const SEEDS: Record<SiteTile, number> = {
  grass_top: 3,
  grass_side: 5,
  dirt: 7,
  stone: 11,
  cobble: 13,
  gravel: 17,
  sand: 19,
  planks: 23,
  log_side: 29,
  log_top: 31,
  birch_side: 37,
  birch_top: 41,
  plaster: 43,
  roof: 47,
  leaves: 53,
  leaves_birch: 59,
  leaves_pine: 61,
  obsidian: 67,
  deepslate: 71,
  ore_copper: 73,
  ore_amethyst: 79,
  crystal: 83,
  lantern: 89,
  window: 97,
  metal: 101,
};

function baseColor(kind: SiteTile): Rgb {
  switch (kind) {
    case "grass_top":
      return [226, 232, 214];
    case "grass_side":
      return [198, 184, 158];
    case "dirt":
      return [190, 172, 144];
    case "stone":
      return [182, 186, 182];
    case "cobble":
      return [188, 188, 178];
    case "gravel":
      return [184, 176, 156];
    case "sand":
      return [230, 218, 188];
    case "planks":
      return [202, 178, 140];
    case "log_side":
      return [174, 140, 102];
    case "log_top":
      return [196, 164, 122];
    case "birch_side":
      return [232, 230, 218];
    case "birch_top":
      return [222, 216, 196];
    case "plaster":
      return [242, 236, 222];
    case "roof":
      return [172, 134, 112];
    case "leaves":
      return [186, 204, 158];
    case "leaves_birch":
      return [204, 218, 166];
    case "leaves_pine":
      return [128, 158, 124];
    case "obsidian":
      return [92, 84, 112];
    case "deepslate":
      return [106, 110, 114];
    case "ore_copper":
      return [196, 196, 188];
    case "ore_amethyst":
      return [196, 196, 188];
    case "crystal":
      return [176, 244, 226];
    case "lantern":
      return [255, 218, 146];
    case "window":
      return [255, 224, 170];
    case "metal":
      return [126, 132, 130];
  }
}

function pixel(kind: SiteTile, x: number, y: number): Rgb {
  const seed = SEEDS[kind];
  const base = baseColor(kind);
  const n = hash(x, y, seed);
  const coarse = hash(x >> 2, y >> 2, seed + 3);
  const fine = hash(x, y, seed + 7);
  let tone = 0.93 + coarse * 0.1 + (fine - 0.5) * 0.05;
  let rgb: Rgb = [base[0] * tone, base[1] * tone, base[2] * tone];

  switch (kind) {
    case "grass_top": {
      if (hash(x >> 1, y >> 1, seed + 11) > 0.82) tone -= 0.07;
      if (n > 0.93) tone += 0.05;
      rgb = [base[0] * tone, base[1] * tone + 3, base[2] * tone];
      break;
    }
    case "grass_side": {
      const green = y >= 12 || (y >= 9 && hash(x, 0, seed + 13) > 0.58);
      if (green) {
        const grass = 0.9 + coarse * 0.16 + (fine - 0.5) * 0.08;
        rgb = [128 * grass, 170 * grass, 92 * grass];
      } else {
        const dirt = 0.92 + coarse * 0.12 + (fine - 0.5) * 0.06;
        rgb = [198 * dirt, 184 * dirt, 158 * dirt];
        if (fine > 0.9) rgb = rgb.map((value) => value * 0.9) as Rgb;
      }
      break;
    }
    case "stone": {
      if ((x * 3 + y) % 17 === 0 || (x + y * 2) % 29 === 0) tone -= 0.1;
      if (fine > 0.95) tone += 0.07;
      rgb = [base[0] * tone, base[1] * tone, base[2] * tone];
      break;
    }
    case "cobble": {
      const cell = hash(x >> 2, y >> 2, seed + 5);
      const border = x % 4 === 0 || y % 4 === 0;
      if (border) {
        tone = 0.74 + cell * 0.08;
      } else {
        tone = 0.93 + cell * 0.14 + (fine - 0.5) * 0.05;
        if (y % 4 === 1) tone += 0.04;
      }
      rgb = [base[0] * tone, base[1] * tone, base[2] * tone];
      break;
    }
    case "gravel": {
      const pebble = hash(x >> 1, y >> 1, seed + 9);
      tone = 0.82 + pebble * 0.22 + (fine - 0.5) * 0.06;
      rgb = [base[0] * tone, base[1] * tone, base[2] * tone];
      break;
    }
    case "sand": {
      if (fine > 0.9) tone -= 0.05;
      rgb = [base[0] * tone, base[1] * tone, base[2] * tone];
      break;
    }
    case "planks": {
      const joint = y < 8 ? 3 : 11;
      if (y % 4 === 0) tone = 0.8 + coarse * 0.05;
      else if (x === joint) tone = 0.86;
      else if (hash(x, y >> 1, seed + 4) > 0.84) tone -= 0.05;
      rgb = [base[0] * tone, base[1] * tone, base[2] * tone];
      break;
    }
    case "log_side": {
      const column = hash(x, 0, seed);
      tone = 0.9 + column * 0.14 + (fine - 0.5) * 0.05;
      if (hash(x, y >> 2, seed + 8) > 0.78) tone -= 0.08;
      rgb = [base[0] * tone, base[1] * tone, base[2] * tone];
      break;
    }
    case "log_top": {
      const dx = x - 7.5;
      const dy = y - 7.5;
      const radius = Math.sqrt(dx * dx + dy * dy);
      const ring = Math.sin(radius * 2.2) > 0.25 ? 0.88 : 1.02;
      tone = ring + (fine - 0.5) * 0.04;
      rgb = [base[0] * tone, base[1] * tone, base[2] * tone];
      break;
    }
    case "birch_side": {
      if (hash(x >> 1, y >> 1, seed + 6) > 0.84) {
        const mark = 0.55 + coarse * 0.1;
        rgb = [base[0] * mark, base[1] * mark, base[2] * mark];
      } else {
        rgb = [base[0] * tone, base[1] * tone, base[2] * tone];
      }
      break;
    }
    case "birch_top": {
      const dx = x - 7.5;
      const dy = y - 7.5;
      const radius = Math.sqrt(dx * dx + dy * dy);
      tone = Math.sin(radius * 2.4) > 0.3 ? 0.9 : 1.03;
      rgb = [base[0] * tone, base[1] * tone, base[2] * tone];
      break;
    }
    case "plaster": {
      if (fine > 0.94) tone -= 0.06;
      rgb = [base[0] * tone, base[1] * tone, base[2] * tone];
      break;
    }
    case "roof": {
      const row = y % 4;
      const cell = Math.floor(y / 4);
      const offset = (cell % 2) * 2;
      if (row === 0) tone = 0.84;
      else if (row === 1) tone = 1.04;
      else tone = 0.95 + coarse * 0.05;
      if (x % 5 === offset) tone *= 0.88;
      rgb = [base[0] * tone, base[1] * tone, base[2] * tone];
      break;
    }
    case "leaves":
    case "leaves_birch":
    case "leaves_pine": {
      const cluster = hash(x >> 1, y >> 1, seed + 10);
      if (cluster < 0.06) tone = 0.87 + fine * 0.03;
      else if (cluster > 0.8) tone = 1.02 + fine * 0.02;
      else tone = 0.96 + cluster * 0.04;
      rgb = [base[0] * tone, base[1] * tone, base[2] * tone];
      break;
    }
    case "obsidian": {
      if (fine > 0.9) {
        const fleck = 0.75 + coarse * 0.22;
        rgb = [104 * fleck, 84 * fleck, 136 * fleck];
      } else {
        tone = 0.7 + coarse * 0.18;
        rgb = [base[0] * tone, base[1] * tone, base[2] * tone];
      }
      break;
    }
    case "deepslate": {
      if (hash(x, y >> 2, seed + 12) > 0.8) tone -= 0.08;
      if (fine > 0.96) tone += 0.06;
      rgb = [base[0] * tone, base[1] * tone, base[2] * tone];
      break;
    }
    case "ore_copper": {
      const blob = hash(x >> 1, y >> 1, seed + 14);
      if (blob > 0.82) {
        const glow = 0.8 + fine * 0.2;
        rgb = [184 * glow, 126 * glow, 84 * glow];
      } else {
        rgb = [base[0] * tone, base[1] * tone, base[2] * tone];
      }
      break;
    }
    case "ore_amethyst": {
      const blob = hash(x >> 1, y >> 1, seed + 15);
      if (blob > 0.82) {
        const glow = 0.78 + fine * 0.22;
        rgb = [146 * glow, 100 * glow, 196 * glow];
      } else {
        rgb = [base[0] * tone, base[1] * tone, base[2] * tone];
      }
      break;
    }
    case "crystal": {
      const facet = Math.sin((x + y) * 0.9) * 0.5 + 0.5;
      const vertical = 0.86 + (y / SIZE) * 0.28;
      rgb = [
        (132 + facet * 70) * vertical,
        (222 + facet * 28) * vertical,
        (206 + facet * 40) * vertical,
      ];
      break;
    }
    case "lantern": {
      const frame = x === 0 || y === 0 || x === 15 || y === 15;
      if (frame) rgb = [96, 82, 62];
      else {
        const glow =
          0.9 + Math.max(0, 1 - Math.hypot(x - 7.5, y - 7.5) / 9) * 0.3;
        rgb = [255 * glow, 216 * glow, 146 * glow];
      }
      break;
    }
    case "window": {
      const frame = x === 0 || y === 0 || x === 15 || y === 15;
      const bar = x === 7 || x === 8 || y === 8;
      if (frame) rgb = [92, 78, 60];
      else if (bar) rgb = [118, 96, 68];
      else {
        const glow =
          0.92 + Math.max(0, 1 - Math.hypot(x - 7.5, y - 7.5) / 10) * 0.24;
        rgb = [255 * glow, 224 * glow, 164 * glow];
      }
      break;
    }
    case "metal": {
      if (y === 7 || y === 8) tone += 0.14;
      if ((x === 3 || x === 12) && (y === 3 || y === 12)) tone -= 0.16;
      rgb = [base[0] * tone, base[1] * tone, base[2] * tone];
      break;
    }
    default:
      break;
  }

  return [byte(rgb[0]), byte(rgb[1]), byte(rgb[2])];
}

function createTile(kind: SiteTile, anisotropy: number) {
  const pixels = new Uint8Array(SIZE * SIZE * 4);
  for (let y = 0; y < SIZE; y++) {
    for (let x = 0; x < SIZE; x++) {
      const [r, g, b] = pixel(kind, x, y);
      const offset = (y * SIZE + x) * 4;
      pixels[offset] = r;
      pixels[offset + 1] = g;
      pixels[offset + 2] = b;
      pixels[offset + 3] = 255;
    }
  }
  const texture = new DataTexture(
    pixels,
    SIZE,
    SIZE,
    RGBAFormat,
    UnsignedByteType,
  );
  texture.name = `ml-site-${kind}`;
  texture.colorSpace = SRGBColorSpace;
  texture.magFilter = NearestFilter;
  texture.minFilter = NearestMipmapLinearFilter;
  texture.generateMipmaps = true;
  texture.anisotropy = Math.min(4, anisotropy);
  texture.wrapS = texture.wrapT = ClampToEdgeWrapping;
  texture.needsUpdate = true;
  return texture;
}

export class TextureBank {
  private textures = new Map<SiteTile, DataTexture>();

  constructor(private anisotropy = 1) {}

  get(kind: SiteTile) {
    const existing = this.textures.get(kind);
    if (existing) return existing;
    const texture = createTile(kind, this.anisotropy);
    this.textures.set(kind, texture);
    return texture;
  }

  dispose() {
    for (const texture of this.textures.values()) texture.dispose();
    this.textures.clear();
  }
}
