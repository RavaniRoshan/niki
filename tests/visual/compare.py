#!/usr/bin/env python3
"""Reference-frame diff gate for tests/visual.

Compares frames/<name>.png against reference/<name>.png per pixel, counting
pixels whose max channel delta exceeds NOISE_FLOOR. Fails files above
FAIL_PCT and writes diff overlays to frames/diff-<name>.png.

Calibration rationale: two consecutive captures of the same tape on the same
machine differ only by cursor-blink phase and subpixel AA jitter — measured
<0.2% on this rig. The 1.5% gate leaves 7x headroom for font/akk cross-machine
variance while catching any real layout/color/glyph regression (which moves
5-40% of pixels). If CI fonts differ systematically, recalibrate: run twice,
read the self-diff from the log, set FAIL_PCT to 5x self-diff.
"""
import os
import sys
from PIL import Image, ImageChops

NOISE_FLOOR = 12  # per-channel delta below this is AA/cursor noise
FAIL_PCT = 1.5


def diff_pct(new_path, ref_path):
    a = Image.open(new_path).convert("RGB")
    b = Image.open(ref_path).convert("RGB")
    if a.size != b.size:
        return 100.0, None
    diff = ImageChops.difference(a, b)
    hist = diff.histogram()
    total = a.size[0] * a.size[1]
    # histogram() concatenates R+G+B channels; a pixel "differs" if any
    # channel exceeds the floor — approximate via per-channel counts.
    diff_pixels = 0
    for ch in range(3):
        band = hist[ch * 256:(ch + 1) * 256]
        diff_pixels = max(diff_pixels, sum(band[NOISE_FLOOR + 1:]))
    return 100.0 * diff_pixels / total, diff


def main(frames_dir, ref_dir):
    failures = []
    checked = 0
    for name in sorted(os.listdir(frames_dir)):
        if not name.endswith(".png") or name.startswith("diff-"):
            continue
        new_path = os.path.join(frames_dir, name)
        ref_path = os.path.join(ref_dir, name)
        if not os.path.exists(ref_path):
            print(f"  NEW      {name} (no reference — bless with REGEN=1 after review)")
            continue
        pct, diff = diff_pct(new_path, ref_path)
        checked += 1
        flag = "FAIL" if pct > FAIL_PCT else "ok"
        print(f"  {flag:<4}   {name} {pct:.2f}% pixels differ")
        if pct > FAIL_PCT:
            failures.append(name)
            if diff is not None:
                # Amplify the diff for human review.
                amp = diff.point(lambda v: min(255, v * 4))
                amp.save(os.path.join(frames_dir, f"diff-{name}"))
    print(f"checked={checked} failures={len(failures)}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1], sys.argv[2]))
