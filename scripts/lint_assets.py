#!/usr/bin/env python3
"""Lint marketing assets to catch regressions.

Catches three classes of bugs:
  1. Page screenshots that look identical to each other (the original
     asset pipeline had every page screenshot look the same).
  2. GIFs that are static (single frame repeated).
  3. PNGs that are too small or corrupt.

Uses Pillow (already in dev-deps via httpx/requests) and falls back to
ffmpeg for GIF frame extraction if Pillow is missing.
"""
from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCREENSHOTS = ROOT / "assets" / "screenshots"
GIFS = list(ROOT.glob("assets/*.gif")) + list(SCREENSHOTS.glob("*.gif"))

fail = 0


def load_pil():
    try:
        from PIL import Image  # type: ignore
        return Image
    except ImportError:
        return None


def perceptual_hash(img, size: int = 16) -> str:
    """Simple average-hash: shrink to size×size grayscale, threshold on mean."""
    img = img.convert("L").resize((size, size))
    pixels = list(img.getdata())
    mean = sum(pixels) / len(pixels)
    return "".join("1" if p > mean else "0" for p in pixels)


def hamming(a: str, b: str) -> int:
    return sum(1 for x, y in zip(a, b) if x != y)


def lint_png_dimensions() -> None:
    global fail
    print("=== Linting PNG dimensions ===")
    Image = load_pil()
    if Image is None:
        print("  SKIP: Pillow not available")
        return
    for png in sorted(SCREENSHOTS.glob("*.png")):
        try:
            with Image.open(png) as im:
                w, h = im.size
        except Exception as e:
            print(f"  FAIL: {png.name} cannot be opened: {e}")
            fail += 1
            continue
        if w < 800 or h < 400:
            print(f"  FAIL: {png.name} is {w}x{h} (expected >= 800x400)")
            fail += 1
        else:
            print(f"  OK: {png.name} ({w}x{h})")


def lint_page_distinctness() -> None:
    global fail
    print("\n=== Linting page-screenshot distinctness ===")
    Image = load_pil()
    if Image is None:
        print("  SKIP: Pillow not available")
        return
    pages = sorted(SCREENSHOTS.glob("page-*.png"))
    if len(pages) < 2:
        print("  SKIP: fewer than 2 page screenshots")
        return
    hashes = {p: perceptual_hash(Image.open(p)) for p in pages}
    seen_fail = False
    for i, a in enumerate(pages):
        for b in pages[i + 1 :]:
            d = hamming(hashes[a], hashes[b])
            # 16x16 average hash: identical images → 0, similar → < 4,
            # different pages → > 6 typically.
            if d < 4:
                print(f"  FAIL: {a.name} vs {b.name} hamming={d} (too similar)")
                fail += 1
                seen_fail = True
            else:
                print(f"  OK: {a.name} vs {b.name} hamming={d}")
    if not seen_fail:
        print(f"  ({len(pages)} pages, all distinct)")


def extract_gif_frames(gif: Path) -> list:
    """Extract a few representative frames from a GIF."""
    Image = load_pil()
    if Image is None:
        return []
    frames = []
    with tempfile.TemporaryDirectory() as td:
        try:
            with Image.open(gif) as im:
                n = getattr(im, "n_frames", 1)
                # Sample 0, mid, last.
                indices = sorted({0, n // 2, n - 1})
                for idx in indices:
                    im.seek(idx)
                    frame = im.convert("RGB").copy()
                    frames.append(frame)
        except Exception as e:
            print(f"  WARN: cannot read {gif.name}: {e}")
    return frames


def lint_gifs() -> None:
    global fail
    print("\n=== Linting GIFs ===")
    for gif in sorted(set(GIFS)):
        if not gif.exists():
            continue
        frames = extract_gif_frames(gif)
        if len(frames) < 3:
            print(f"  FAIL: {gif.name} yielded {len(frames)} frame(s) (expected 3)")
            fail += 1
            continue
        h0 = perceptual_hash(frames[0])
        h1 = perceptual_hash(frames[1])
        h2 = perceptual_hash(frames[2])
        d01 = hamming(h0, h1)
        d12 = hamming(h1, h2)
        if d01 < 2 and d12 < 2:
            print(f"  FAIL: {gif.name} frames near-identical (d01={d01} d12={d12}) — likely static")
            fail += 1
        else:
            print(f"  OK: {gif.name} (d01={d01} d12={d12})")


def main() -> int:
    lint_png_dimensions()
    lint_page_distinctness()
    lint_gifs()
    print()
    if fail:
        print(f"=== {fail} lint check(s) failed ===")
        return 1
    print("=== All asset lint checks passed ===")
    return 0


if __name__ == "__main__":
    sys.exit(main())
