#!/usr/bin/env python3
"""Generate placeholder TCG card template PNGs (frame + colour-keyed mask)."""

from PIL import Image, ImageDraw

W, H = 500, 700
BORDER = 16
CORNER_R = 20

# Region definitions (in pixels)
TITLE_TOP = BORDER
TITLE_H = 44
TITLE_RECT = (BORDER, TITLE_TOP, W - BORDER, TITLE_TOP + TITLE_H)

ART_TOP = TITLE_TOP + TITLE_H
ART_BOTTOM = int(H * 0.58)
ART_RECT = (BORDER, ART_TOP, W - BORDER, ART_BOTTOM)

TYPE_TOP = ART_BOTTOM
TYPE_H = 28
TYPE_RECT = (BORDER, TYPE_TOP, W - BORDER, TYPE_TOP + TYPE_H)

DESC_TOP = TYPE_TOP + TYPE_H
STATS_H = 40
DESC_BOTTOM = H - BORDER - STATS_H
DESC_RECT = (BORDER, DESC_TOP, W - BORDER, DESC_BOTTOM)

STATS_TOP = DESC_BOTTOM
STATS_RECT = (BORDER, STATS_TOP, W - BORDER, H - BORDER)

OUT_DIR = "assets/templates/default"

# ========== MASK ==========
mask = Image.new("RGBA", (W, H), (0, 0, 0, 255))
md = ImageDraw.Draw(mask)

md.rectangle(TITLE_RECT, fill=(255, 0, 0, 255))      # Red = title
md.rectangle(ART_RECT, fill=(0, 255, 0, 255))         # Green = art
md.rectangle(TYPE_RECT, fill=(255, 0, 255, 255))      # Magenta = type_line
md.rectangle(DESC_RECT, fill=(0, 0, 255, 255))        # Blue = description
md.rectangle(STATS_RECT, fill=(255, 255, 0, 255))     # Yellow = stats

mask.save(f"{OUT_DIR}/mask.png")
print(f"mask.png: {W}x{H}")

# ========== FRAME ==========
frame = Image.new("RGBA", (W, H), (0, 0, 0, 0))
fd = ImageDraw.Draw(frame)

# Outer border
border_color = (45, 45, 65, 255)
fd.rounded_rectangle((0, 0, W - 1, H - 1), radius=CORNER_R, fill=border_color)

# Cut out inner area (transparent where art shows through)
inner_rect = (BORDER, BORDER, W - BORDER, H - BORDER)
inner_r = max(CORNER_R - BORDER // 2, 4)
fd.rounded_rectangle(inner_rect, radius=inner_r, fill=(0, 0, 0, 0))

# Title bar overlay (semi-transparent dark)
fd.rectangle(TITLE_RECT, fill=(20, 20, 38, 210))

# Type line overlay
fd.rectangle(TYPE_RECT, fill=(25, 25, 42, 200))

# Description area overlay
fd.rectangle(DESC_RECT, fill=(18, 18, 34, 220))

# Stats bar overlay
fd.rectangle(STATS_RECT, fill=(22, 22, 40, 215))

# Separator lines (muted gold accent)
accent = (180, 150, 80, 150)
fd.line([(BORDER + 8, TYPE_TOP), (W - BORDER - 8, TYPE_TOP)], fill=accent, width=1)
fd.line([(BORDER + 8, STATS_TOP), (W - BORDER - 8, STATS_TOP)], fill=accent, width=1)

# Subtle inner border around art window
fd.rectangle(
    (BORDER - 1, ART_TOP, W - BORDER, ART_BOTTOM - 1),
    outline=(60, 60, 80, 180),
    width=1,
)

frame.save(f"{OUT_DIR}/frame.png")
print(f"frame.png: {W}x{H}")
print("Done!")
