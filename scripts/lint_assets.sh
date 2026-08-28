#!/usr/bin/env bash
# Lint marketing assets to catch regressions.
#
# 1. Every page screenshot must be perceptually distinct from every other
#    page screenshot (catches the "all pages look identical" bug).
# 2. Every GIF must contain at least 3 distinct frames.
# 3. Every PNG must be non-zero size and the right dimensions.
#
# Uses ImageMagick `compare` and `identify`. Exits non-zero on any failure.

set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v magick >/dev/null 2>&1 && ! command -v convert >/dev/null 2>&1; then
    echo "ImageMagick not found. Install with: sudo apt install imagemagick"
    exit 1
fi

if command -v magick >/dev/null 2>&1; then
    CONVERT="magick convert"
    IDENTIFY="magick identify"
    COMPARE="magick compare"
else
    CONVERT="convert"
    IDENTIFY="identify"
    COMPARE="compare"
fi

fail=0

echo "=== Linting PNG dimensions ==="
for png in assets/screenshots/*.png; do
    [ -f "$png" ] || continue
    size=$($IDENTIFY -format "%w %h" "$png" 2>/dev/null || echo "0 0")
    w=$(echo "$size" | awk '{print $1}')
    h=$(echo "$size" | awk '{print $2}')
    if [ "$w" -lt 800 ] || [ "$h" -lt 400 ]; then
        echo "  FAIL: $png has dimensions ${w}x${h} (expected ≥ 800x400)"
        fail=$((fail + 1))
    else
        echo "  OK: $png (${w}x${h})"
    fi
done

echo
echo "=== Linting page-screenshot distinctness ==="
# All page screenshots must be mutually distinct.
shopt -s nullglob
pages=(assets/screenshots/page-*.png)
shopt -u nullglob
if [ ${#pages[@]} -lt 2 ]; then
    echo "  SKIP: fewer than 2 page screenshots"
else
    for ((i = 0; i < ${#pages[@]}; i++)); do
        for ((j = i + 1; j < ${#pages[@]}; j++)); do
            a="${pages[i]}"
            b="${pages[j]}"
            # Use AE (absolute error) metric; we want high distance between
            # distinct pages, low distance if they're the same. 0 = identical.
            diff=$($COMPARE -metric AE -fuzz 5% "$a" "$b" /dev/null 2>&1 || true)
            # Numeric threshold: < 2000 means visually near-identical.
            if [ "${diff:-0}" -lt 2000 ] 2>/dev/null; then
                echo "  FAIL: $(basename "$a") and $(basename "$b") are near-identical (AE=$diff)"
                fail=$((fail + 1))
            else
                echo "  OK: $(basename "$a") vs $(basename "$b") (AE=$diff)"
            fi
        done
    done
fi

echo
echo "=== Linting GIFs (frame distinctness) ==="
shopt -s nullglob
gifs=(assets/*.gif assets/screenshots/*.gif)
shopt -u nullglob
for gif in "${gifs[@]}"; do
    [ -f "$gif" ] || continue
    frames=$($IDENTIFY "$gif" 2>/dev/null | wc -l)
    if [ "$frames" -lt 5 ]; then
        echo "  FAIL: $gif has only $frames frame(s) (expected ≥ 5)"
        fail=$((fail + 1))
    else
        # Sample first, middle, last frame and check pairwise distance.
        # The check uses ImageMagick compare on extracted frames.
        work=$(mktemp -d)
        $CONVERT "$gif[0]" "$work/frame-0.png" 2>/dev/null
        mid=$((frames / 2))
        $CONVERT "$gif[$mid]" "$work/frame-mid.png" 2>/dev/null
        last=$((frames - 1))
        $CONVERT "$gif[$last]" "$work/frame-last.png" 2>/dev/null
        d1=$($COMPARE -metric AE -fuzz 5% "$work/frame-0.png" "$work/frame-mid.png" /dev/null 2>&1 || true)
        d2=$($COMPARE -metric AE -fuzz 5% "$work/frame-mid.png" "$work/frame-last.png" /dev/null 2>&1 || true)
        rm -rf "$work"
        if [ "${d1:-0}" -lt 200 ] 2>/dev/null && [ "${d2:-0}" -lt 200 ] 2>/dev/null; then
            echo "  FAIL: $gif frames too similar (d1=$d1 d2=$d2) — animation may be static"
            fail=$((fail + 1))
        else
            echo "  OK: $gif ($frames frames, d1=$d1 d2=$d2)"
        fi
    fi
done

echo
if [ $fail -ne 0 ]; then
    echo "=== $fail lint check(s) failed ==="
    exit 1
fi
echo "=== All asset lint checks passed ==="
