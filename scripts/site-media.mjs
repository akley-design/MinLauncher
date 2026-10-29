import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = fileURLToPath(new URL("../", import.meta.url));
const assets = path.join(root, "packages/visuals/assets");
const media = path.join(root, "apps/site/public/media");
const og = path.join(root, "apps/site/public/og");
const publicDir = path.join(root, "apps/site/public");

mkdirSync(media, { recursive: true });
mkdirSync(og, { recursive: true });

const variants = ["island", "cave", "portal"];
const gravity = { island: "East", cave: "West", portal: "Center" };

function magick(args) {
  execFileSync("magick", args, { stdio: "inherit" });
}

function convert(source, target, args) {
  magick([source, ...args, target]);
}

for (const variant of variants) {
  const source = path.join(assets, `${variant}-site.png`);

  for (const size of [480, 720, 960]) {
    convert(source, path.join(media, `${variant}-${size}.webp`), [
      "-resize",
      `${size}x${size}`,
      "-quality",
      "82",
      "-define",
      "webp:alpha-quality=100",
    ]);
    convert(source, path.join(media, `${variant}-${size}.avif`), [
      "-resize",
      `${size}x${size}`,
      "-quality",
      "55",
    ]);
  }

  for (const [width, height, resize] of [
    [960, 540, "x506"],
    [1440, 810, "x760"],
  ]) {
    for (const [format, quality] of [
      ["webp", 88],
      ["avif", 58],
    ]) {
      convert(source, path.join(media, `${variant}-wide-${width}.${format}`), [
        "-trim",
        "+repage",
        "-resize",
        resize,
        "-background",
        "none",
        "-gravity",
        gravity[variant],
        "-extent",
        `${width}x${height}`,
        "-quality",
        String(quality),
        ...(format === "webp" ? ["-define", "webp:alpha-quality=100"] : []),
      ]);
    }
  }

  for (const [width, height, resize] of [
    [480, 300, "x276"],
    [720, 450, "x414"],
  ]) {
    for (const [format, quality] of [
      ["webp", 86],
      ["avif", 56],
    ]) {
      convert(source, path.join(media, `${variant}-cover-${width}.${format}`), [
        "-trim",
        "+repage",
        "-resize",
        resize,
        "-background",
        "none",
        "-gravity",
        "Center",
        "-extent",
        `${width}x${height}`,
        "-quality",
        String(quality),
        ...(format === "webp" ? ["-define", "webp:alpha-quality=100"] : []),
      ]);
    }
  }

  convert(source, path.join(og, `article-${variant}.jpg`), [
    "-trim",
    "+repage",
    "-resize",
    "x630",
    "-background",
    "#141716",
    "-gravity",
    gravity[variant],
    "-extent",
    "1200x630",
    "-alpha",
    "remove",
    "-alpha",
    "off",
    "-quality",
    "82",
  ]);
}

for (const [name, variant] of [
  ["home", "island"],
  ["features", "cave"],
  ["download", "portal"],
  ["news", "portal"],
]) {
  copyFileSync(
    path.join(og, `article-${variant}.jpg`),
    path.join(og, `${name}.jpg`),
  );
}

const preview = path.join(root, "apps/ui/public/launcher-preview.png");
for (const [width, height] of [
  [960, 618],
  [1180, 760],
]) {
  for (const [format, quality] of [
    ["webp", 82],
    ["avif", 52],
  ]) {
    convert(preview, path.join(media, `launcher-preview-${width}.${format}`), [
      "-resize",
      `${width}x${height}`,
      "-quality",
      String(quality),
      ...(format === "webp" ? ["-define", "webp:alpha-quality=100"] : []),
    ]);
  }
}

convert(
  path.join(root, "apps/ui/public/brand.svg"),
  path.join(publicDir, "brand-512.png"),
  [
    "-background",
    "none",
    "-resize",
    "512x512",
    "-depth",
    "8",
    "-strip",
    "-define",
    "png:compression-level=9",
  ],
);

console.log("Site media generated in apps/site/public/media and /og");
