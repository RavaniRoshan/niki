#!/usr/bin/env python3
"""Reference-frame diff gate for tests/visual.

Compares frames/<name>.png against reference/<name>.png per pixel, counting
pixels where ANY channel delta exceeds NOISE_FLOOR. Fails files above FAIL_PCT
and writes diff overlays to frames/diff-<name>.png.

Three ways this gate used to pass while comparing nothing useful, all fixed:

1. `ImageChops.difference(...).histogram()` concatenates per-channel bins, so
   the union of "pixels differing in R, G or B" cannot be recovered from it.
   The old code took `max` over the three per-channel counts, under-reporting
   by up to 3x — a colour change touching all three channels of 0.6% of
   pixels reported as 0.6% and passed a 1.5% gate. Now a per-pixel maximum is
   computed explicitly.
2. A frame with no reference was printed as NEW and skipped without failing.
   Deleting all 12 reference PNGs produced `checked=0 failures=0` and a green
   `visual` job. A missing reference is now a failure.
3. `checked == 0` passed. It is now a failure, so the gate can never report
   success having compared nothing.

Calibration rationale: two consecutive captures of the same tape on the same
machine differ only by cursor-blink phase and subpixel AA jitter — measured
<0.2% on this rig. The 1.5% gate leaves headroom for font/AA cross-machine
variance while catching any real layout/color/glyph regression (which moves
5-40% of pixels). If CI fonts differ systematically, recalibrate: run twice,
read the self-diff from the log, set FAIL_PCT to 5x self-diff.
"""
import os
import sys
from PIL import Image, ImageChops

NOISE_FLOOR = 12  # per-channel delta below this is AA/cursor noise
FAIL_PCT = 1.5


def differing_pixels(new_path, ref_path):
    """Count pixels where any channel delta exceeds NOISE_FLOOR.

    Returns (percentage, diff_image_or_None). A size mismatch is a 100%
    difference — a resized frame is a layout regression, not noise.
    """
    a = Image.open(new_path).convert("RGB")
    b = Image.open(ref_path).convert("RGB")
    if a.size != b.size:
        return 100.0, None

    diff = ImageChops.difference(a, b)
    total = a.size[0] * a.size[1]

    # Per-pixel max across channels, then threshold. `histogram()` cannot do
    # this because it discards pixel identity.
    r, g, bl = diff.split()
    peak = ImageChops.lighter(ImageChops.lighter(r, g), bl)
    mask = peak.point(lambda v: 255 if v > NOISE_FLOOR else 0)
    changed = sum(count for value, count in enumerate(mask.histogram()) if value > 0)
    return 100.0 * changed / total, diff


def main(frames_dir, ref_dir):
    failures = []
    missing_refs = []
    checked = 0

    names = sorted(
        n for n in os.listdir(frames_dir) if n.endswith(".png") and not n.startswith("diff-")
    )
    if not names:
        print("FAIL     no frames were captured — the capture step produced nothing")
        return 1

    for name in names:
        new_path = os.path.join(frames_dir, name)
        ref_path = os.path.join(ref_dir, name)
        if not os.path.exists(ref_path):
            print(f"  FAIL   {name} has no reference frame")
            missing_refs.append(name)
            continue
        pct, diff = differing_pixels(new_path, ref_path)
        checked += 1
        flag = "FAIL" if pct > FAIL_PCT else "ok"
        print(f"  {flag:<4}   {name} {pct:.2f}% pixels differ")
        if pct > FAIL_PCT:
            failures.append(name)
            if diff is not None:
                amp = diff.point(lambda v: min(255, v * 4))
                amp.save(os.path.join(frames_dir, f"diff-{name}"))

    print(f"checked={checked} failures={len(failures)} missing_references={len(missing_refs)}")

    if checked == 0:
        print(
            "FAIL     zero frames were compared — the gate must never report success "
            "having checked nothing"
        )
        return 1
    if missing_refs:
        print(
            "FAIL     missing reference frames: {}. Bless them deliberately with "
            "REGEN=1 ./run.sh after reviewing the PNGs.".format(", ".join(missing_refs))
        )
    return 1 if (failures or missing_refs) else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1], sys.argv[2]))
