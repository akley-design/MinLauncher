#!/usr/bin/env python3
"""Generate the MinLauncher application icons.

The icon is resized from ``crates/ml-desktop/icons/icon-source.png``
(the block-built M mark), so rerunning this script
reproduces every derived file.

Usage:
    python scripts/generate_icons.py <output-dir>

Produces:
    32x32.png, 128x128.png, 128x128@2x.png, icon.ico
"""

from __future__ import annotations

import os
import pathlib
import sys

from PIL import Image

SIZES = {
    "32x32.png": 32,
    "128x128.png": 128,
    "128x128@2x.png": 256,
}

SOURCE_NAME = "icon-source.png"


def repo_root() -> pathlib.Path:
    return pathlib.Path(__file__).resolve().parents[1]


def load_source(output_dir: str) -> Image.Image:
    candidates = [
        pathlib.Path(output_dir) / SOURCE_NAME,
        repo_root() / "crates" / "ml-desktop" / "icons" / SOURCE_NAME,
    ]
    for candidate in candidates:
        if candidate.is_file():
            image = Image.open(candidate)
            # Normalize to square RGBA; source is already square.
            image.load()
            return image.convert("RGBA")
    searched = ", ".join(str(path) for path in candidates)
    raise FileNotFoundError(f"icon source not found (searched: {searched})")


def resized(source: Image.Image, size: int) -> Image.Image:
    return source.resize((size, size), Image.LANCZOS)


def main() -> int:
    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} <output-dir>", file=sys.stderr)
        return 2

    output_dir = sys.argv[1]
    os.makedirs(output_dir, exist_ok=True)

    source = load_source(output_dir)

    for name, size in SIZES.items():
        path = os.path.join(output_dir, name)
        resized(source, size).save(path, "PNG", optimize=True)
        print(f"wrote {path} ({size}x{size})")

    # The .ico bundles several sizes so that Windows can pick the one that
    # matches the display context.
    ico_path = os.path.join(output_dir, "icon.ico")
    largest = resized(source, 256)
    largest.save(
        ico_path,
        "ICO",
        sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )
    print(f"wrote {ico_path}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
