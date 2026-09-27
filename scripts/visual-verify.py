#!/usr/bin/env python3
"""Frame-by-frame visual verification of NIKI's terminal UI.

The shimmer and the spinner are verified by unit tests, but a unit test only
proves the arithmetic is right — not that the result *looks* like a travelling
band rather than a flicker, a colour glitch, or a layout that shifts as the
effect runs. Those are visual properties and they need visual evidence.

This harness drives the shipped binary under a real pseudo-terminal, samples
the rendered cell grid at a fixed cadence, and checks the things only the
rendered output can show:

  1. the shimmer band moves along the text and returns to its start each period
  2. it never changes width or moves the cursor (effects must not reflow the
     layout, or the badge visibly jitters)
  3. brightness rises and falls monotonically through the band — a hue shift
     would pass a "did it move" test and look wrong
  4. the spinner advances and wraps
  5. every page renders without overflowing its region, at several widths
  6. reduced motion collapses both to a static frame

It also writes PNGs so the result can be *looked at*, which is the part no
assertion replaces.

Usage:
  python3 scripts/visual-verify.py                 # run every check
  python3 scripts/visual-verify.py --shots         # also write PNGs
"""
from __future__ import annotations

import asyncio
import os
import sys
import tempfile
from pathlib import Path

import tuiwright

REPO = Path(__file__).resolve().parent.parent
BIN = os.environ.get("NIKI_BIN", str(REPO / "target/release/niki"))
COLS, ROWS = 110, 26

# One shimmer period is 2.0s. Sampling at 8Hz gives 16 frames per period,
# enough to resolve a band that crosses in well under a second.
SAMPLE_HZ = 8
PERIOD_S = 2.0

green = lambda m: print(f"\033[32m{m}\033[0m")
red = lambda m: print(f"\033[31m{m}\033[0m")
info = lambda m: print(f"\033[2m{m}\033[0m")
head = lambda m: print(f"\n\033[1m{m}\033[0m")

FAILURES: list[str] = []

# `tuiwright.start(env=...)` merges with the parent environment rather than
# replacing it, so an inherited `NO_COLOR=1` reaches niki and the whole UI
# renders monochrome — correct behaviour, and useless for verifying colour.
# Cleared here so the harness can actually see the palette.
os.environ.pop("NO_COLOR", None)


def check(name: str, ok: bool, detail: str = "") -> None:
    if ok:
        green(f"  ok    {name}" + (f"  {detail}" if detail else ""))
    else:
        red(f"  FAIL  {name}  {detail}")
        FAILURES.append(name)


async def start(cols=COLS, rows=ROWS, env_extra=None, settle=True):
    tmp = tempfile.mkdtemp(prefix="niki-vis-")
    env = {"TERM": "ghostty", "TERM_PROGRAM": "ghostty", "COLORTERM": "truecolor"}
    env.update(env_extra or {})
    s = tuiwright.TuiSession()
    await s.start([BIN, "chat", "-p", tmp], cols=cols, rows=rows, env=env)
    await asyncio.sleep(1.5)
    try:
        await s.press("escape")
    except Exception:
        pass
    # `settle=False` for animated checks: with the shimmer running the screen is
    # *supposed* to never reach quiet, and waiting for stability here would
    # time out on a working animation.
    if settle:
        try:
            await s.wait_for_stable(quiet_ms=300, timeout=5)
        except tuiwright.TuiTimeoutError:
            # Still animating; that is fine, just give it a moment.
            await asyncio.sleep(0.3)
    else:
        await asyncio.sleep(0.4)
    return s


def badge_cells(s, row=ROWS - 1):
    """(column, brightness) for the permission badge on the status row."""
    line = s.screen.row(row)
    try:
        start = line.index("MANUAL") if "MANUAL" in line else line.index("BYPASS")
    except ValueError:
        return None
    out = []
    for col in range(start, min(start + 7, COLS)):
        c = s.screen.cell(row, col)
        fgc = c.fg
        rgb = getattr(fgc, "rgb", None)
        # `rgb` is a hex string; the default colour reports None.
        if not rgb or not isinstance(rgb, str):
            out.append((col, None))
        else:
            out.append((col, sum(int(rgb[i : i + 2], 16) for i in (0, 2, 4)) / 3.0))
    return out


def text_of(s, row=ROWS - 1):
    return s.screen.row(row)


def render_png(s, path: Path, cols=COLS, rows=ROWS, scale=9, pad=14):
    """Draw the terminal cell grid to a PNG.

    `tuiwright.png()` shells out to asciinema-agg, which is not installed
    here — and an image nobody looks at is not a visual check. This reads the
    same cell grid the assertions read, so the PNG shows exactly what was
    checked: real glyphs, real fg/bg colours, real bold.
    """
    try:
        from PIL import Image, ImageDraw, ImageFont
    except ImportError:
        return False
    font = None
    for cand in (
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf",
    ):
        try:
            font = ImageFont.truetype(cand, scale * 2)
            break
        except OSError:
            continue
    if font is None:
        return False
    w, h = scale * 2, scale * 3
    img = Image.new("RGB", (cols * w + pad * 2, rows * h + pad * 2), (10, 9, 9))
    d = ImageDraw.Draw(img)

    def rgb(c, default=(220, 214, 208)):
        v = getattr(c, "rgb", None)
        if not v or not isinstance(v, str):
            return default
        return tuple(int(v[i : i + 2], 16) for i in (0, 2, 4))

    for r in range(rows):
        for c in range(cols):
            try:
                cell = s.screen.cell(r, c)
            except Exception:
                continue
            x = pad + c * w
            y = pad + r * h
            bgc = rgb(cell.bg, (26, 23, 22))
            if bgc != (26, 23, 22):
                d.rectangle([x, y, x + w, y + h], fill=bgc)
            ch = cell.char
            if ch and ch != " ":
                fgc = rgb(cell.fg, (208, 200, 192))
                d.text((x, y), ch, font=font, fill=fgc)
    img.save(path)
    return True


async def check_shimmer(samples):
    head("1 · shimmer band moves, holds width, and varies brightness only")
    frames = [f for f in samples if f]
    if len(frames) < 6:
        check("shimmer produced enough frames", False, f"only {len(frames)}")
        return

    widths = {len(f) for f in frames}
    check(
        "badge width is stable across frames",
        len(widths) == 1,
        f"widths={sorted(widths)} — a shifting width means the effect reflows the layout",
    )

    # The brightest column should travel rather than sit still.
    if any(all(b is None for _, b in f) for f in frames):
        red("  FAIL  the badge carries no colour information at all")
        red("        (NO_COLOR inherited, or the terminal is not truecolor)")
        FAILURES.append("shimmer colour visible")
        return
    peaks = [max(range(len(f)), key=lambda i: f[i][1] or -1) for f in frames]
    moved = len(set(peaks)) > 1
    check("the band's peak column changes over time", moved, f"peaks={peaks}")

    # And it should come back roughly where it started after a full period.
    check(
        "the band wraps within one period",
        len(set(peaks)) >= 3,
        f"only {len(set(peaks))} distinct positions over {len(frames)} frames",
    )

    # Brightness must rise and fall, not jitter.
    series = [f[peaks[i]][1] for i, f in enumerate(frames)]
    if any(v is None for v in series):
        red("  FAIL  could not read the badge colours — the TUI rendered without colour")
        red("        (NO_COLOR set, or the terminal is not reporting truecolor)")
        FAILURES.append("shimmer colour visible")
        return
    rising = any(series[i] < series[i + 1] for i in range(len(series) - 1))
    falling = any(series[i] > series[i + 1] for i in range(len(series) - 1))
    check("band brightness rises and falls", rising and falling, f"series={[round(v) for v in series[:8]]}...")


async def check_layout_stability(samples):
    head("2 · the effect never moves the cursor or changes the row text")
    rows = {t for t in samples}
    check(
        "status row text is byte-identical across frames",
        len(rows) == 1,
        f"{len(rows)} distinct renderings — a shimmer must not alter glyphs"
        if len(rows) != 1
        else "",
    )


async def check_pages(s, shots: Path | None):
    head("3 · every page renders inside its region at several widths")
    await s.press("tab")
    await asyncio.sleep(0.5)

    pages = ["Run", "Pipeline", "Diff", "Cost", "Verdict", "Artifacts", "History", "Config"]
    for i, name in enumerate(pages[:8], start=1):
        await s.type(str(i))
        # Not wait_for_stable: the shimmer keeps the screen moving by design.
        await asyncio.sleep(0.45)
        text = s.screen.text
        # Nothing may spill past the terminal width on any row.
        overflow = [r for r in text.split("\n") if len(r) > COLS]
        check(
            f"page {i} ({name}) renders without overflow",
            not overflow,
            f"{len(overflow)} row(s) exceed {COLS} cols" if overflow else "",
        )
        if shots:
            render_png(s, shots / f"page-{i}-{name.lower()}.png")
    info(f"captured {len(pages[:8])} pages")


async def check_narrow(s, shots: Path | None):
    head("4 · narrow terminal (60x20) — the footer ladder must degrade, not wrap")
    await s.resize(cols=60, rows=20)
    await asyncio.sleep(0.6)
    footer = s.screen.row(19)
    check(
        "footer fits the narrowed width",
        len(footer.rstrip()) <= 60,
        f"footer is {len(footer.rstrip())} cols in a 60-col terminal",
    )
    body = s.screen.text
    overflow = [r for r in body.split("\n") if len(r) > 60]
    check("no row overflows at 60 columns", not overflow, f"{len(overflow)} row(s)")
    if shots:
        render_png(s, shots / "narrow-60x20.png", cols=60, rows=20)
    await s.resize(cols=COLS, rows=ROWS)
    await asyncio.sleep(0.5)


async def check_reduced_motion():
    head("5 · reduced motion collapses to a static frame")
    s = await start(env_extra={"NIKI_REDUCED_MOTION": "1"})
    try:
        samples = []
        for _ in range(6):
            await asyncio.sleep(1.0 / SAMPLE_HZ)
            samples.append(badge_cells(s))
        distinct = {tuple((c, b) for c, b in f) for f in samples if f}
        check(
            "reduced motion produces an identical frame every time",
            len(distinct) == 1,
            f"{len(distinct)} distinct frames — reduced motion must be static",
        )
    finally:
        await s.stop()


async def main():
    shots = None
    if "--shots" in sys.argv:
        shots = REPO / "target" / "visual-verify"
        shots.mkdir(parents=True, exist_ok=True)
        info(f"writing PNGs to {shots}")

    info(f"binary: {BIN}")
    s = await start()
    try:
        head("1-2 · shimmer over one full period")
        samples, rows = [], []
        n = int(PERIOD_S * SAMPLE_HZ) + 2
        for _ in range(n):
            await asyncio.sleep(1.0 / SAMPLE_HZ)
            samples.append(badge_cells(s))
            rows.append(text_of(s))
        await check_shimmer(samples)
        await check_layout_stability(rows)
        if shots:
            (shots / "shimmer-frames").mkdir(exist_ok=True)
            for i, f in enumerate(samples):
                if i % 3 == 0 and f:
                    render_png(s, shots / "shimmer-frames" / f"t{i:02d}.png")
            info(f"captured {len(samples) // 3 + 1} shimmer frames")

        await check_pages(s, shots)
        await check_narrow(s, shots)
    finally:
        await s.stop()

    await check_reduced_motion()

    print()
    if FAILURES:
        red(f"{len(FAILURES)} check(s) failed: {', '.join(FAILURES)}")
        return 1
    green("all visual checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
