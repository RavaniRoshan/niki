#!/usr/bin/env python3
"""Render Niki TUI cell-buffer JSON (from src/bin/render_tui.rs) to PNG."""
import sys, json
from PIL import Image, ImageFont

FONT = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"
FONT_BOLD = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf"
SCALE = 2; CELL_W = 9; CELL_H = 18

def load_font(p, s):
    try: return ImageFont.truetype(p, s)
    except: return ImageFont.load_default()

with open(sys.argv[1]) as f: data = json.load(f)
cells, cols, rows = data["cells"], data["width"], data["height"]
W, H = cols*CELL_W*SCALE, rows*CELL_H*SCALE
img = Image.new("RGB", (W, H), (13, 13, 13))
draw = ImageDraw.Draw(img)
font, fb = load_font(FONT, 14*SCALE), load_font(FONT_BOLD, 14*SCALE)
for ry, row in enumerate(cells):
    for cx, cell in enumerate(row):
        x, y = cx*CELL_W*SCALE, ry*CELL_H*SCALE
        bg, fg = cell["bg"], cell["fg"]
        ch = cell["ch"]
        draw.rectangle([x, y, x+CELL_W*SCALE, y+CELL_H*SCALE], fill=(bg["r"],bg["g"],bg["b"]))
        if ch and ch != " ":
            f = fb if cell.get("bold") else font
            try: tw, th = draw.textbbox((0,0), ch, font=f)[2:]
            except: tw, th = 9*SCALE, 14*SCALE
            draw.text((x+(CELL_W*SCALE)//2-tw//2, y+(CELL_H*SCALE)//2-th//2+2), ch, font=f, fill=(fg["r"],fg["g"],fg["b"]))
img.save(sys.argv[2])
print(f"Saved {sys.argv[2]}: {W}x{H}")
