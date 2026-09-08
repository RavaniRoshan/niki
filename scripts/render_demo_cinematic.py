#!/usr/bin/env python3
"""
NIKI cinematic demo renderer v2 — the REAL Niki TUI inside a macOS window.

Content replicates `niki chat` pixel-for-pixel from src/display source:
compact header (logo.rs), chat_log roles (pages/chat.rs), collapsed stage
headers + previews, tool cards (components/tool_card.rs), permission modal
(components/permission.rs), rounded input capsule with pills
(components/input_box.rs), status bar (components/status_bar.rs).

Only the outer chrome (macOS traffic-lights window on wallpaper) is borrowed
from the Claude Code reference demo's *style*; every terminal pixel is NIKI.

Canvas 1280x840 @ 10fps, 800 frames = 80 s. Frame-buffered PIL rendering =>
zero flicker. Transcript is bottom-anchored like a real terminal and scrolls
under a pixel mask, so nothing ever overlaps.

Usage:
    python -B scripts/render_demo_cinematic.py --preview-only   # stills check
    python -B scripts/render_demo_cinematic.py                  # MP4 + GIF
"""

from __future__ import annotations

import argparse
import os
import subprocess

import imageio_ffmpeg
import numpy as np
from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont

# ── canvas / window geometry ──────────────────────────────────────────────
W, H = 1280, 840
FPS = 10
TOTAL = 800  # 80 s

WIN_X, WIN_Y, WIN_W, WIN_H = 36, 36, 1208, 768
TITLE_H = 46
RADIUS = 14
BODY_Y = WIN_Y + TITLE_H
BODY_BOT = WIN_Y + WIN_H

TEXT_X = WIN_X + 30
TEXT_W = WIN_W - 60
TEXT_R = TEXT_X + TEXT_W

FONT_SIZE = 16
ROW_H = 25
TRACKING = 1

# header rows (real compact header + welcome block)
H0_Y = BODY_Y + 28
H1_Y = H0_Y + ROW_H
H2_Y = H1_Y + ROW_H
H3_Y = H2_Y + ROW_H
HEADER_RULE_Y = H3_Y + ROW_H + 4

# transcript scroll zone
ZONE_TOP = HEADER_RULE_Y + 8
T0_Y = ZONE_TOP + 6

# pinned input capsule (3 rows) + status bar (1 row)
IB_TOP_Y = BODY_BOT - 122
IB_MID_Y = IB_TOP_Y + ROW_H
IB_BOT_Y = IB_MID_Y + ROW_H
STATUS_Y = IB_BOT_Y + ROW_H + 6
ZONE_BOT = IB_TOP_Y - 8

# permission modal box
MODAL_COLS = 66
MODAL_ROWS = 19

# ── palette: NIKI token.md Tier-1 + theme.rs dark ─────────────────────────
BODY_BG   = (26, 23, 22)     # ESPRESSO_800 bg.canvas
ELEVATED  = (32, 29, 29)     # ESPRESSO_700 bg.modal
HIGHLIGHT = (40, 36, 35)     # ESPRESSO_600 bg.input
DEEP      = (20, 18, 17)     # ESPRESSO_900 pills
BORDER    = (56, 51, 48)     # ESPRESSO_500 border.subtle
BORDER_DIM = (46, 42, 40)    # theme border_dim (input idle)
CREAM     = (250, 248, 245)  # CREAM_100 text.hero
BRIGHT    = (243, 239, 234)  # CREAM_200 text.body
DIMC      = (230, 223, 213)  # CREAM_300 text.dim
SUBTLE    = (138, 132, 128)  # CREAM_500 text.muted
CLAY      = (204, 120, 92)   # CLAY_500 brand.primary
SAND      = (212, 163, 115)  # SAND_500 planner (token.md Tier-3)
AMBER     = (224, 159, 62)   # AMBER_500 reviewer
TGREEN    = (78, 190, 130)   # THINKING_GREEN spinners
SUCCESS   = (52, 211, 153)   # SUCCESS_GREEN checkmarks
ERROR     = (231, 111, 81)   # ERROR_CORAL failures
INFO      = (106, 155, 204)  # INFO_BLUE planner per chat.rs theme::sand()
PURPLE    = (150, 130, 200)  # coder per chat.rs theme::purple
TITLE_BG  = (244, 241, 236)
TITLE_FG  = (60, 60, 62)

# real chat.rs role colors (local role_color fn) + working-tree theme fix
# (sand() -> warm SAND_500; branch badge -> INFO_BLUE per token.md)
ROLE = {
    "planner": SAND,
    "coder": CLAY,
    "tester": TGREEN,
    "reviewer": AMBER,
    "user": CLAY,
    "assistant": SAND,
    "system": SUBTLE,
}
ASSISTANT_C = SAND
BRANCH_C = INFO
SPINNER = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]

# ── fonts (exact cmap-based picker; tofu impossible) ──────────────────────
FONTS_DIR = r"C:\Windows\Fonts"


def _cmap(path: str) -> set:
    try:
        from fontTools.ttLib import TTFont
        return set(TTFont(path, lazy=True).getBestCmap().keys())
    except Exception:
        return set()


_CONSOLA = _cmap(os.path.join(FONTS_DIR, "consola.ttf"))
_CONSOLAB = _cmap(os.path.join(FONTS_DIR, "consolab.ttf"))
_SEGUISYM = _cmap(os.path.join(FONTS_DIR, "seguisym.ttf"))
_KNOWN = _CONSOLA | _CONSOLAB | _SEGUISYM
_FONT_CACHE: dict = {}


def _load(name: str, size: int) -> ImageFont.FreeTypeFont:
    return ImageFont.truetype(os.path.join(FONTS_DIR, name), size)


def get_font(ch: str, size: int = FONT_SIZE, bold: bool = False):
    key = (ch, size, bold)
    f = _FONT_CACHE.get(key)
    if f is not None:
        return f
    o = ord(ch)
    if bold and o in _CONSOLAB:
        f = _load("consolab.ttf", size)
    elif o in _CONSOLA:
        f = _load("consola.ttf", size)
    elif o in _SEGUISYM:
        f = _load("seguisym.ttf", size)
    else:
        raise RuntimeError(f"no font covers U+{o:04X} ({ch!r})")
    _FONT_CACHE[key] = f
    return f


def seg_width(segs) -> int:
    w = 0
    for text, _color, bold in segs:
        for ch in text:
            w += int(get_font(ch, FONT_SIZE, bold).getlength(ch)) + TRACKING
    return w


def assert_glyphs(*seg_lists) -> None:
    for segs in seg_lists:
        for text, _c, _b in segs:
            for ch in text:
                assert ord(ch) in _KNOWN, f"uncovered char U+{ord(ch):04X} ({ch!r})"


def draw_segs(dr: ImageDraw.ImageDraw, x: int, y: int, segs) -> int:
    for text, color, bold in segs:
        for ch in text:
            f = get_font(ch, FONT_SIZE, bold)
            dr.text((x, y), ch, font=f, fill=color)
            x += int(f.getlength(ch)) + TRACKING
    return x


def adv(ch: str, size: int = FONT_SIZE, bold: bool = False) -> int:
    f = get_font(ch, size, bold)
    return int(f.getlength(ch)) + TRACKING


def draw_hborder(dr, x0: int, x1: int, y: int, lch: str, rch: str,
                 fill: str, color, size: int = FONT_SIZE) -> None:
    """Pixel-exact horizontal border: corners pinned to x0/x1, fill tiled
    between (uniform glyphs hide sub-cell overlap, so no gaps ever)."""
    fl, fr, ff = get_font(lch, size, False), get_font(rch, size, False), get_font(fill, size, False)
    lw, rw, fw = adv(lch, size), adv(rch, size), adv(fill, size)
    dr.text((x0, y), lch, font=fl, fill=color)
    dr.text((x1 - rw, y), rch, font=fr, fill=color)
    span = (x1 - rw) - (x0 + lw)
    n = max(1, round(span / fw))
    step = span / n
    for i in range(n):
        dr.text((x0 + lw + i * step, y), fill, font=ff, fill=color)


def L(*parts) -> list:
    return list(parts)


# ── wallpaper + chrome (reference style only) ─────────────────────────────
def make_wallpaper() -> Image.Image:
    yy, xx = np.mgrid[0:H, 0:W].astype(np.float32)
    u, v = xx / W, yy / H
    diag = u * 0.7 + v * 0.5
    r = 90 + 150 * np.clip(np.sin(diag * 5.2 + 0.6) * 0.5 + 0.5, 0, 1) * (0.35 + 0.65 * u)
    g = 70 + 90 * np.clip(np.sin(diag * 4.1 + 2.0) * 0.5 + 0.5, 0, 1)
    b = 150 + 60 * np.clip(np.sin(diag * 3.0 + 4.0) * 0.5 + 0.5, 0, 1) * (0.4 + 0.6 * (1 - u))
    warm = np.exp(-(((u - 0.30) ** 2) / 0.06 + ((v - 0.12) ** 2) / 0.10))
    r, g, b = r + warm * 60, g + warm * 30, b - warm * 40
    blue = np.exp(-(((u - 0.95) ** 2) / 0.08 + ((v - 0.55) ** 2) / 0.35))
    r, g, b = r - blue * 70, g - blue * 30, b + blue * 40
    arr = np.stack([np.clip(r, 0, 255), np.clip(g, 0, 255), np.clip(b, 0, 255)], -1).astype(np.uint8)
    img = Image.fromarray(arr, "RGB").filter(ImageFilter.GaussianBlur(6))
    from PIL import ImageEnhance
    return ImageEnhance.Brightness(ImageEnhance.Color(img).enhance(1.22)).enhance(1.04)


def round_mask(w: int, h: int, r: int) -> Image.Image:
    m = Image.new("L", (w, h), 0)
    ImageDraw.Draw(m).rounded_rectangle([0, 0, w - 1, h - 1], radius=r, fill=255)
    return m


def make_base() -> Image.Image:
    wp = make_wallpaper()
    base = wp.copy()
    sh = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    ImageDraw.Draw(sh).rounded_rectangle(
        [WIN_X - 2, WIN_Y + 8, WIN_X + WIN_W + 2, WIN_Y + WIN_H + 14],
        radius=RADIUS + 4, fill=(0, 0, 0, 110))
    base = Image.alpha_composite(base.convert("RGBA"), sh.filter(ImageFilter.GaussianBlur(14)))

    win = Image.new("RGBA", (WIN_W, WIN_H), BODY_BG + (255,))
    tdr = ImageDraw.Draw(win)
    tdr.rounded_rectangle([0, 0, WIN_W - 1, TITLE_H + 6], radius=RADIUS, fill=TITLE_BG + (255,))
    tdr.rectangle([0, TITLE_H - 8, WIN_W, TITLE_H + 6], fill=TITLE_BG + (255,))
    for i, col in enumerate([(255, 95, 87), (255, 189, 46), (40, 202, 66)]):
        cx, cy = 30 + i * 22, TITLE_H // 2
        tdr.ellipse([cx - 7, cy - 7, cx + 7, cy + 7], fill=col + (255,))
    title = "◈ NIKI · Fix /health"
    tw = sum(int(get_font(c, 15, True).getlength(c)) + 1 for c in title)
    x = (WIN_W - tw) // 2
    for ch in title:
        f = get_font(ch, 15, ch not in "·/")
        tdr.text((x, 13), ch, font=f, fill=TITLE_FG + (255,))
        x += int(f.getlength(ch)) + 1
    tr = "⌐⌘2"
    trw = sum(int(get_font(c, 15, False).getlength(c)) + 1 for c in tr)
    x = WIN_W - trw - 24
    for ch in tr:
        f = get_font(ch, 15, False)
        tdr.text((x, 13), ch, font=f, fill=TITLE_FG + (255,))
        x += int(f.getlength(ch)) + 1

    base.paste(win, (WIN_X, WIN_Y), round_mask(WIN_W, WIN_H, RADIUS))
    base = base.convert("RGB")
    dr = ImageDraw.Draw(base)

    # real compact header (logo.rs) + welcome block (pages/chat.rs)
    draw_segs(dr, TEXT_X, H0_Y, L(("◈ ", CLAY, False), ("NIKI ", CREAM, True),
                                  ("v0.4.0", SUBTLE, False), (" · flawed-app (main)", DIMC, False)))
    draw_segs(dr, TEXT_X, H1_Y, L(("✦ Welcome to NIKI", BRIGHT, False)))
    draw_segs(dr, TEXT_X, H2_Y, L(("  chat session", BRIGHT, False)))
    draw_segs(dr, TEXT_X, H3_Y, L(("  Directory: /home/shiva/projects/flawed-app", BRIGHT, False)))
    dr.line([TEXT_X, HEADER_RULE_Y, TEXT_R, HEADER_RULE_Y], fill=BORDER, width=1)
    return base


# ── transcript rows: (key, appear, stream_len, until) ─────────────────────
# Dynamic rows (spinners) resolve segments per-frame via DYN.
PROMPT_TXT = "fix /health: returns 500 when the DB is down"
ASSIST_TXT = "I'll fix the /health endpoint and prove it with tests."
ASSIST2_TXT = "Done — branch niki/a7f3c2 · report.md · changes.patch"

STATIC = {
    "b1": L(), "b2": L(), "b3": L(),
    "user": L(("◈ ", CLAY, False), ("user: ", CLAY, True), (PROMPT_TXT, CREAM, False)),
    "blank": L(),
    "assist": L(("⟠ ", ASSISTANT_C, False), ("assistant: ", ASSISTANT_C, True), (ASSIST_TXT, BRIGHT, False)),
    "plan_done": L((" ▸ ", SUBTLE, False), ("✓ ", SUCCESS, False), ("◈ ", SAND, False),
                   ("Planner   ", SAND, True), ("812 tok · $0.0011", SUBTLE, False)),
    "plan_prev": L(("  └ TaskSpec ready · 2 files to touch", SUBTLE, False)),
    "read_hdr": L(("  ", SUBTLE, False), ("✓ ", SUCCESS, False), ("Read ", CREAM, True),
                  ("src/routes/health.ts", DIMC, False)),
    "read_out": L(("    31 lines", DIMC, False)),
    "read_time": L(("  ─ 18ms", SUBTLE, False)),
    "coder_done": L((" ▸ ", SUBTLE, False), ("✓ ", SUCCESS, False), ("⟠ ", CLAY, False),
                    ("Coder     ", CLAY, True), ("640 tok · $0.0009", SUBTLE, False)),
    "coder_prev": L(("  └ unified diff applied · +18 -4", SUBTLE, False)),
    "bash_hdr": L(("  ", SUBTLE, False), ("✗ ", ERROR, False), ("Bash ", CREAM, True),
                  ("npx vitest run tests/health.test.ts", DIMC, False)),
    "bash_out": L(("    1 failed · GET /health → 500 when DB down", DIMC, False)),
    "bash_time": L(("  ─ 210ms", SUBTLE, False)),
    "test_done": L((" ▸ ", SUBTLE, False), ("✓ ", SUCCESS, False), ("◉ ", TGREEN, False),
                   ("Tester    ", TGREEN, True), ("388 tok · $0.0004", SUBTLE, False)),
    "test_prev": L(("  └ 8/8 passed · 210ms", SUBTLE, False)),
    "rev_done": L((" ▸ ", SUBTLE, False), ("✓ ", SUCCESS, False), ("◆ ", AMBER, False),
                  ("Reviewer  ", AMBER, True), ("512 tok · $0.0007", SUBTLE, False)),
    "rev_prev": L(("  └ Approved · correctness 10/10 · quality 8/10", SUBTLE, False)),
    "assist2": L(("⟠ ", ASSISTANT_C, False), ("assistant: ", ASSISTANT_C, True), (ASSIST2_TXT, BRIGHT, False)),
    "cost0": L(("◆ ", SUBTLE, False), ("system: ", SUBTLE, True), ("Session Economics:", BRIGHT, False)),
    "cost1": L(("     • Total Spend:  $0.0041 USD", BRIGHT, False)),
    "cost2": L(("     • Input Tokens:  2,180", BRIGHT, False)),
    "cost3": L(("     • Output Tokens: 640", BRIGHT, False)),
    "cost4": L(("     • Cache Read:   16,800 (92.4% hit)", BRIGHT, False)),
    "cost5": L(("     • Model: claude-sonnet-4-20250514", BRIGHT, False)),
}

# key -> (appear, stream_chars, until_or_None)
BEATS = {
    "user": (135, 0, None), "b1": (135, 0, None),
    "assist": (140, 55, None), "b2": (140, 0, None),
    "plan_run": (305, 0, 360),
    "plan_done": (360, 0, None), "plan_prev": (360, 0, None),
    "read_run": (368, 0, 388),
    "read_hdr": (388, 0, None), "read_out": (388, 0, None), "read_time": (388, 0, None),
    "coder_run": (400, 0, 470),
    "coder_done": (470, 0, None), "coder_prev": (470, 0, None),
    "bash_run": (478, 0, 505),
    "bash_hdr": (505, 0, None), "bash_out": (505, 0, None), "bash_time": (505, 0, None),
    "test_run": (512, 0, 560),
    "test_done": (560, 0, None), "test_prev": (560, 0, None),
    "rev_run": (568, 0, 615),
    "rev_done": (615, 0, None), "rev_prev": (615, 0, None),
    "assist2": (622, 60, None), "b3": (622, 0, None),
    "cost0": (730, 0, None), "cost1": (738, 0, None), "cost2": (744, 0, None),
    "cost3": (750, 0, None), "cost4": (756, 0, None), "cost5": (762, 0, None),
}
ORDER = ["user", "b1", "assist", "b2",
         "plan_run", "plan_done", "plan_prev",
         "read_run", "read_hdr", "read_out", "read_time",
         "coder_run", "coder_done", "coder_prev",
         "bash_run", "bash_hdr", "bash_out", "bash_time",
         "test_run", "test_done", "test_prev",
         "rev_run", "rev_done", "rev_prev",
         "assist2", "b3",
         "cost0", "cost1", "cost2", "cost3", "cost4", "cost5"]

RUN_STYLE = {  # running stage/card -> (glyph_color, icon, icon_color, label, label_color, verb)
    "plan_run": (CLAY, "◈", SAND, "Planner", SAND, "Formulating plan"),
    "read_run": (CLAY, None, None, "Read", None, "src/routes/health.ts"),
    "coder_run": (CLAY, "⟠", CLAY, "Coder", CLAY, "Synthesizing solution"),
    "bash_run": (CLAY, None, None, "Bash", None, "npx vitest run tests/health.test.ts"),
    "test_run": (CLAY, "◉", TGREEN, "Tester", TGREEN, "Running verification"),
    "rev_run": (CLAY, "◆", AMBER, "Reviewer", AMBER, "Auditing diffs"),
}


def run_segs(key: str, f: int):
    gcol, icon, icol, label, lcol, verb = RUN_STYLE[key]
    spin = SPINNER[(f // 2) % len(SPINNER)]
    if icon is None:  # tool card running: `  ⠋ Bash <summary>`
        return L(("  ", SUBTLE, False), (spin + " ", gcol, True),
                 (label + " ", CREAM, True), (verb, DIMC, False))
    # stage running: ` ▾ ⠋ ◈ Planner   ∴ Formulating plan...`
    pad = " " * (10 - len(label))
    return L((" ▾ ", SUBTLE, False), (spin + " ", gcol, True), (icon + " ", icol, False),
             ((label + pad), lcol, True), (f"∴ {verb}...", TGREEN, False))


def row_segs(key: str, f: int):
    if key in RUN_STYLE:
        return run_segs(key, f)
    return STATIC[key]


def row_len(key: str) -> int:
    if key in RUN_STYLE:
        _, icon, _, label, _, verb = RUN_STYLE[key]
        if icon is None:
            return len(f"  ⠋ {label} {verb}")
        return len(f" ▾ ⠋ {icon} {label}" + " " * (10 - len(label)) + f"∴ {verb}...")
    return sum(len(t) for t, _c, _b in STATIC[key])


def visible_rows(f: int):
    return [k for k in ORDER if BEATS[k][0] <= f
            and (BEATS[k][2] is None or f < BEATS[k][2])]


def visible_chars(key: str, f: int) -> int:
    appear, stream, _u = BEATS[key]
    if f < appear:
        return 0
    if stream == 0:
        return row_len(key)
    return min(row_len(key), int((f - appear) * row_len(key) / stream) + 1)


def truncate_segs(segs, n: int):
    out, left = [], n
    for text, color, bold in segs:
        if left <= 0:
            break
        out.append((text[:left], color, bold))
        left -= len(text[:left])
    return out


# ── timeline: input box + modal + status ──────────────────────────────────
PROMPT_TYPE = (30, 110)
SUBMIT = 135
COST_TYPE = (690, 725)
COST_SUBMIT = 725
MODAL = (232, 292)
STATUS_EXTRAS = 672  # branch/cost/ctx appear
RUNNING_SPAN = (305, 615)  # input capsule dimmed (has_running_stage)


def state_at(f: int):
    if f < PROMPT_TYPE[0]:
        itext, cursor = "", True
    elif f < PROMPT_TYPE[1]:
        n = min(len(PROMPT_TXT), int((f - PROMPT_TYPE[0]) * len(PROMPT_TXT)
                                     / (PROMPT_TYPE[1] - PROMPT_TYPE[0])) + 1)
        itext, cursor = PROMPT_TXT[:n], True
    elif f < COST_SUBMIT:
        itext, cursor = (PROMPT_TXT if f < SUBMIT else ""), True
    elif f < COST_TYPE[1]:
        n = min(5, int((f - COST_TYPE[0]) * 5 / (COST_TYPE[1] - COST_TYPE[0])) + 1)
        itext, cursor = "/cost"[:n], True
    else:
        itext, cursor = "", True

    shown = visible_rows(f) if f >= SUBMIT else []
    n = len(shown)
    scroll = max(0, (T0_Y + n * ROW_H) - (ZONE_BOT + ROW_H))

    modal = MODAL[0] <= f < MODAL[1]
    streaming = RUNNING_SPAN[0] <= f < RUNNING_SPAN[1]
    extras = f >= STATUS_EXTRAS
    fade = min(1.0, f / 19) if f < 20 else 1.0
    if f >= TOTAL - 15:
        fade = min(fade, max(0.0, (TOTAL - 1 - f) / 14))
    return shown, itext, (cursor and (f % 10) < 6), scroll, modal, streaming, extras, fade


# ── input capsule (components/input_box.rs, Rounded) ──────────────────────
PLACEHOLDER = "Describe a change or press / for commands..."


def draw_input_box(img: Image.Image, text: str, cursor_on: bool, streaming: bool) -> None:
    dr = ImageDraw.Draw(img, "RGBA")
    bcol = DIMC if streaming else BORDER_DIM
    bg = BODY_BG if streaming else HIGHLIGHT
    inner_l, inner_r = TEXT_X + 1, TEXT_R - 1
    dr.rectangle([TEXT_X + 2, IB_TOP_Y + 2, TEXT_R - 2, IB_BOT_Y + ROW_H - 4], fill=bg + (255,))
    draw_hborder(dr, TEXT_X, TEXT_R, IB_TOP_Y, "╭", "╮", "─", bcol + (255,))
    draw_hborder(dr, TEXT_X, TEXT_R, IB_BOT_Y, "╰", "╯", "─", bcol + (255,))
    # vertical edges on all three rows
    for yy in (IB_TOP_Y, IB_MID_Y, IB_BOT_Y):
        for xe in (TEXT_X, TEXT_R):
            f = get_font("│", FONT_SIZE, False)
            if xe == TEXT_R:
                xe2 = TEXT_R - (int(f.getlength("│")) + TRACKING)
            else:
                xe2 = xe
            dr.text((xe2, yy), "│", font=f, fill=bcol + (255,))

    # mid row content: `│ ▎ Build <text>█  ...  sandbox  podman │`
    y = IB_MID_Y
    pills = [(" sandbox ", DIMC), (" ", None), (" podman ", DIMC)]
    pill_w = sum(seg_width([(t, DIMC, False)]) for t, c in pills if c) + seg_width([(" ", DIMC, False)])
    cx = inner_l + 2
    cx = draw_segs(dr, cx, y, L((" ", CREAM, False), ("▎", CLAY, False), (" ", CREAM, False),
                                ("Build ", CREAM, True)))
    if text:
        cx = draw_segs(dr, cx, y, L((text, BRIGHT, False)))
        if cursor_on:
            cx = draw_cursor(dr, cx, y)
    else:
        if cursor_on:
            cx = draw_cursor(dr, cx, y)
        cx = draw_segs(dr, cx, y, L((PLACEHOLDER, SUBTLE, False)))
    # right-aligned pills on DEEP chips
    px = inner_r - pill_w - 2
    if px < cx + 4:
        px = cx + 4
    for pt, pc in pills:
        if pc is None:
            px = draw_segs(dr, px, y, L((pt, DIMC, False)))
        else:
            w = seg_width([(pt, DIMC, False)])
            asc, desc = get_font(" ", FONT_SIZE, False).getmetrics()
            dr.rectangle([px, y + 3, px + w, y + 3 + asc + desc - 2], fill=DEEP + (255,))
            px = draw_segs(dr, px, y, L((pt, pc, False)))


def draw_cursor(dr, x: int, y: int, ch: str = " ") -> int:
    f = get_font(ch or " ", FONT_SIZE, False)
    cw = int(f.getlength(ch or " ")) + TRACKING
    asc, desc = f.getmetrics()
    dr.rectangle([x, y + 2, x + cw - TRACKING, y + 2 + asc + desc], fill=CLAY + (255,))
    if ch != " ":
        dr.text((x, y), ch, font=f, fill=(*BODY_BG, 255))
    return x + cw


# ── status bar (components/status_bar.rs) ─────────────────────────────────
def draw_status_bar(img: Image.Image, extras: bool) -> None:
    dr = ImageDraw.Draw(img)
    y = STATUS_Y
    segs = L((" claude-sonne… ", CREAM, True),
             ("tab ", CREAM, True), ("toggle view   ", SUBTLE, False),
             ("ctrl-p ", CREAM, True), ("commands   ", SUBTLE, False),
             ("esc ", CREAM, True), ("quit (run continues)", SUBTLE, False))
    right = []
    if extras:
        right = L(("branch niki/a7f3c2   ", BRANCH_C, False),
                  ("$0.0041   ", DIMC, False),
                  ("ctx ", DIMC, False), ("▓", TGREEN, False),
                  ("░░░░░░░░░", DIMC, False), (" 12%   ", DIMC, False))
    badge = L((" MANUAL ", SUBTLE, True))
    lw, bw = seg_width(segs), seg_width(badge)
    # greedy fit like the real bar: left first, then right groups only if
    # they fit; the ctx gauge is atomic (all-or-nothing) so the bar never
    # renders half-built or collides with the shortcuts.
    groups = []
    if extras:
        groups = [[right[0]], [right[1]], right[2:]]
    used, kept = lw, []
    for g in groups:
        gw = seg_width(g)
        if used + gw + bw <= TEXT_W:
            kept.extend(g)
            used += gw
    rw = seg_width(kept)
    draw_segs(dr, TEXT_X, y, segs)
    draw_segs(dr, TEXT_R - bw, y, badge)
    if kept:
        draw_segs(dr, TEXT_R - bw - rw - 2, y, kept)


# ── permission modal (components/permission.rs, compact 66x19) ────────────
MODAL_CMD = "npx vitest run tests/health.test.ts"
MODAL_DESC = "Run the health suite in the sandbox to reproduce the flaw."


def draw_modal(img: Image.Image) -> None:
    char_w = 11  # avg cell incl tracking at 16px
    bw = MODAL_COLS * char_w
    bh = MODAL_ROWS * ROW_H
    x0 = (W - bw) // 2
    y0 = BODY_Y + (WIN_H - TITLE_H - bh) // 2
    x1, y1 = x0 + bw, y0 + bh
    dr = ImageDraw.Draw(img, "RGBA")
    dr.rectangle([x0 + 4, y0 + 8, x1 + 4, y1 + 10], fill=(0, 0, 0, 120))
    dr.rectangle([x0, y0, x1, y1], fill=ELEVATED + (255,))
    bcol = BORDER + (255,)
    vert = "│"
    title = " Permission Required "
    # top border with embedded title (ratatui Block title style)
    draw_hborder(dr, x0 + 2, x1 - 2, y0, "┌", "┐", "─", bcol)
    xt = x0 + 2 + adv("┌")
    tw = sum(adv(ch, FONT_SIZE, True) for ch in title)
    dr.rectangle([xt, y0, xt + tw, y0 + ROW_H], fill=ELEVATED + (255,))
    for ch in title:
        f = get_font(ch, FONT_SIZE, True)
        dr.text((xt, y0), ch, font=f, fill=CREAM + (255,))
        xt += adv(ch, FONT_SIZE, True)
    # bottom border
    draw_hborder(dr, x0 + 2, x1 - 2, y0 + (MODAL_ROWS - 1) * ROW_H, "└", "┘", "─", bcol)
    # sides (pinned to the same x as the corners)
    vrw = adv(vert)
    for r in range(1, MODAL_ROWS - 1):
        dr.text((x0 + 2, y0 + r * ROW_H), vert,
                font=get_font(vert, FONT_SIZE, False), fill=bcol)
        dr.text((x1 - 2 - vrw, y0 + r * ROW_H), vert,
                font=get_font(vert, FONT_SIZE, False), fill=bcol)
    ty = y0 + ROW_H
    ix = x0 + 16
    draw_segs(dr, ix, ty, L(("The agent wants to run:", DIMC, False)))
    ty += 2 * ROW_H
    draw_segs(dr, ix, ty, L((f"  $ {MODAL_CMD}", CLAY, True)))
    ty += 2 * ROW_H
    draw_hborder(dr, ix, x1 - 16, ty, "─", "─", "─", CLAY + (255,))
    ty += 2 * ROW_H
    draw_segs(dr, ix, ty, L((f"  {MODAL_DESC}", BRIGHT, False)))
    ty += 2 * ROW_H
    draw_segs(dr, ix, ty, L(("  Scope: ", DIMC, False), ("●", CLAY, True), (" Turn  ", CLAY, True),
                            ("○", DIMC, False), (" Session  ", DIMC, False),
                            ("○", DIMC, False), (" Project", DIMC, False)))
    ty += 2 * ROW_H
    draw_segs(dr, ix, ty, L(("  ● Allow once", CLAY, True)))
    ty += ROW_H
    for opt in ("  ○ Allow always", "  ○ Deny", "  ○ Deny always"):
        draw_segs(dr, ix, ty, L((opt, BRIGHT, False)))
        ty += ROW_H
    ty += ROW_H
    draw_segs(dr, ix, ty, L(("[↑/↓] Select  [Enter/Y] Confirm  [Esc/N] Deny", DIMC, False)))


# ── frame ─────────────────────────────────────────────────────────────────
def render_frame(base: Image.Image, wp_only: Image.Image, f: int) -> Image.Image:
    shown, itext, cursor_on, scroll, modal, streaming, extras, fade = state_at(f)
    img = Image.blend(wp_only, base, fade) if fade < 1.0 else base.copy()

    tlayer = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    tdr = ImageDraw.Draw(tlayer)
    for i, key in enumerate(shown):
        y = T0_Y + i * ROW_H - scroll
        if y + ROW_H < ZONE_TOP or y > ZONE_BOT:
            continue
        n = visible_chars(key, f)
        if n <= 0:
            continue
        draw_segs(tdr, TEXT_X, y, truncate_segs(row_segs(key, f), n))
    zone = Image.new("L", (W, H), 0)
    ImageDraw.Draw(zone).rectangle([0, ZONE_TOP, W - 1, ZONE_BOT], fill=255)
    img = img.convert("RGBA")
    img.paste(tlayer, (0, 0), ImageChops.darker(tlayer.split()[3], zone))
    img = img.convert("RGB")

    if modal:
        draw_modal(img)
    draw_input_box(img, itext, cursor_on, streaming)
    draw_status_bar(img, extras)
    return img


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--frames", type=int, default=TOTAL)
    ap.add_argument("--preview-only", action="store_true")
    ap.add_argument("--outdir", default=r"C:\Users\shiva\Downloads\gif\niki\assets")
    args = ap.parse_args()

    all_segs = list(STATIC.values())
    for k in RUN_STYLE:
        all_segs.append(run_segs(k, 0))
    assert_glyphs(*all_segs,
                  L(("◈ NIKI · Fix /health", CREAM, True)),
                  L(("⌐⌘2", CREAM, False)),
                  L((PROMPT_TXT, BRIGHT, False)),
                  L((PLACEHOLDER, SUBTLE, False)),
                  L((" claude-sonne… ", CREAM, True)),
                  L(("[↑/↓] Select  [Enter/Y] Confirm  [Esc/N] Deny", DIMC, False)),
                  L((f"  $ {MODAL_CMD}", CLAY, True)),
                  L((f"  {MODAL_DESC}", BRIGHT, False)),
                  L(("ctx ", DIMC, False), ("▓", TGREEN, False), ("░░░░░░░░░", DIMC, False)))
    for key, segs in STATIC.items():
        assert seg_width(segs) <= TEXT_W, f"line {key} too wide"
    print("glyph + width checks ok", flush=True)

    base = make_base()
    wp_only = make_wallpaper()
    if args.preview_only:
        for f in (0, 60, 150, 260, 340, 430, 500, 590, 650, 720, 760, 784):
            render_frame(base, wp_only, f).save(
                rf"C:\Users\shiva\AppData\Local\Temp\opencode\gifframes\new_{f:04d}.png")
        print("previews written", flush=True)
        return

    ffmpeg = imageio_ffmpeg.get_ffmpeg_exe()
    mp4_path = os.path.join(args.outdir, "demo.mp4")
    cmd = [ffmpeg, "-y", "-f", "rawvideo", "-pix_fmt", "rgb24",
           "-s", f"{W}x{H}", "-r", str(FPS), "-i", "-",
           "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "20",
           "-movflags", "+faststart", mp4_path]
    proc = subprocess.Popen(cmd, stdin=subprocess.PIPE,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        for f in range(args.frames):
            proc.stdin.write(render_frame(base, wp_only, f).tobytes())
            if f % 100 == 0:
                print(f"frame {f}/{args.frames}", flush=True)
    finally:
        proc.stdin.close()
        proc.wait()
    print("mp4 done:", mp4_path, flush=True)

    gif_path = os.path.join(args.outdir, "demo.gif")
    pal = r"C:\Users\shiva\AppData\Local\Temp\opencode\gifframes\palette.png"
    subprocess.run([ffmpeg, "-y", "-i", mp4_path,
                    "-vf", "fps=10,scale=800:-1:flags=lanczos,palettegen", pal],
                   check=True, capture_output=True)
    subprocess.run([ffmpeg, "-y", "-i", mp4_path, "-i", pal,
                    "-lavfi", "fps=10,scale=800:-1:flags=lanczos[x];[x][1:v]paletteuse",
                    gif_path], check=True, capture_output=True)
    print("gif done:", gif_path, flush=True)


if __name__ == "__main__":
    main()
