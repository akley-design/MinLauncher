#!/usr/bin/env python3
"""Generate the MinLauncher application icons.

The icon is drawn from scratch (a rounded launcher "arrow" on a dark tile)
so that the repository stays free of opaque binary blobs: rerunning this
script reproduces every file byte for byte.

Usage:
    python scripts/generate_icons.py <output-dir>

Produces:
    32x32.png, 128x128.png, 128x128@2x.png, icon.ico
"""

from __future__ import annotations

import os
import sys

from PIL import Image, ImageDraw

# Base canvas. Everything is drawn at 4x and downsampled, which is what
# gives the curves clean edges.
SCALE = 4

BACKGROUND = (24, 26, 32, 255)
BACKGROUND_EDGE = (36, 40, 50, 255)
ACCENT = (94, 214, 138, 255)
ACCENT_DIM = (58, 160, 104, 255)

SIZES = {
    "32x32.png": 32,
    "128x128.png": 128,
    "128x128@2x.png": 256,
}


def rounded_rect(draw: ImageDraw.ImageDraw, box, radius, fill=None, outline=None, width=1) -> None:
    draw.rounded_rectangle(box, radius=radius, fill=fill, outline=outline, width=width)


def draw_icon(size: int) -> Image.Image:
    """Draw the icon at `size` pixels square."""
    canvas = size * SCALE
    image = Image.new("RGBA", (canvas, canvas), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)

    margin = canvas * 0.06
    rounded_rect(
        draw,
        (margin, margin, canvas - margin, canvas - margin),
        radius=canvas * 0.22,
        fill=BACKGROUND,
    )
    rounded_rect(
        draw,
        (margin, margin, canvas - margin, canvas - margin),
        radius=canvas * 0.22,
        outline=BACKGROUND_EDGE,
        width=max(1, int(canvas * 0.012)),
    )

    # A blocky "launch arrow" pointing up-right, built from three bars so
    # that it stays legible even at 32 pixels.
    cx = canvas * 0.50
    cy = canvas * 0.52
    unit = canvas * 0.115

    # Shaft.
    draw.rounded_rectangle(
        (cx - unit * 0.42, cy - unit * 0.30, cx + unit * 0.30, cy + unit * 1.25),
        radius=unit * 0.30,
        fill=ACCENT_DIM,
    )

    # Head: a chevron drawn as two bars.
    head = unit * 1.30
    draw.rounded_rectangle(
        (cx - unit * 0.30, cy - unit * 1.05, cx + head * 0.62, cy - unit * 0.42),
        radius=unit * 0.26,
        fill=ACCENT,
    )
    draw.rounded_rectangle(
        (cx + head * 0.10, cy - unit * 0.75, cx + head * 0.62, cy + unit * 0.75),
        radius=unit * 0.26,
        fill=ACCENT,
    )

    return image.resize((size, size), Image.LANCZOS)


def main() -> int:
    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} <output-dir>", file=sys.stderr)
        return 2

    output_dir = sys.argv[1]
    os.makedirs(output_dir, exist_ok=True)

    for name, size in SIZES.items():
        path = os.path.join(output_dir, name)
        draw_icon(size).save(path, "PNG", optimize=True)
        print(f"wrote {path} ({size}x{size})")

    # The .ico bundles several sizes so that Windows can pick the one that
    # matches the display context.
    ico_path = os.path.join(output_dir, "icon.ico")
    largest = draw_icon(256)
    largest.save(
        ico_path,
        "ICO",
        sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )
    print(f"wrote {ico_path}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
