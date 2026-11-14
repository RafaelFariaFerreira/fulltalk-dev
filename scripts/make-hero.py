#!/usr/bin/env python3
"""Composes docs/hero.png from the app screenshots (requires Pillow).

Three columns: the compact window (idle, then recording), the window with the
recent transcriptions expanded, and the minimized microphone (idle and
recording) above the settings window.

Generate the screenshots first:
    scripts/screenshots.sh
"""
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

DOCS = Path(__file__).resolve().parent.parent / "docs"


def shot(name: str) -> Image.Image:
    return Image.open(DOCS / "screenshots" / f"{name}.png").convert("RGBA")


def side_by_side(*images: Image.Image, gap: int = 24, align_right_to: int = 0) -> Image.Image:
    """Images in a row, optionally right-aligned inside a canvas `align_right_to` wide."""
    width = sum(i.width for i in images) + gap * (len(images) - 1)
    row = Image.new("RGBA", (max(width, align_right_to), max(i.height for i in images)), (0, 0, 0, 0))
    x = row.width - width
    for image in images:
        row.alpha_composite(image, (x, 0))
        x += image.width + gap
    return row


settings = shot("settings")
# The minimized app lives in the top-right corner of the screen; echo that here.
minimized = side_by_side(shot("mini"), shot("mini-recording"), align_right_to=settings.width - 48)

# Each column is a list of windows stacked top to bottom.
COLUMNS = [[shot("main"), shot("recording")], [shot("history")], [minimized, settings]]
PAD_X, PAD_Y = 90, 70
GAP_X = 36
# Screenshots carry their own transparent shadow margin, so stacked windows overlap a little.
GAP_Y = -8

heights = [sum(w.height for w in col) + GAP_Y * (len(col) - 1) for col in COLUMNS]
widths = [max(w.width for w in col) for col in COLUMNS]
W = PAD_X * 2 + sum(widths) + GAP_X * (len(COLUMNS) - 1)
H = PAD_Y * 2 + max(heights)

bg = Image.new("RGBA", (W, H))
draw = ImageDraw.Draw(bg)
top, bottom = (91, 63, 228), (160, 150, 250)
for y in range(H):
    t = y / H
    draw.line([(0, y), (W, y)], fill=tuple(int(a + (b - a) * t) for a, b in zip(top, bottom)) + (255,))

glow = Image.new("RGBA", (W, H), (0, 0, 0, 0))
g = ImageDraw.Draw(glow)
g.ellipse([-W // 4, H // 2, W * 3 // 4, H * 3 // 2], fill=(255, 255, 255, 40))
g.ellipse([W // 3, -H // 2, W * 4 // 3, H // 2], fill=(255, 255, 255, 28))
bg = Image.alpha_composite(bg, glow.filter(ImageFilter.GaussianBlur(60)))

x = PAD_X
for column, width, height in zip(COLUMNS, widths, heights):
    y = (H - height) // 2
    for window in column:
        bg.alpha_composite(window, (x + (width - window.width) // 2, y))
        y += window.height + GAP_Y
    x += width + GAP_X

# Rounded corners, like the other README illustrations.
mask = Image.new("L", (W, H), 0)
ImageDraw.Draw(mask).rounded_rectangle([0, 0, W - 1, H - 1], radius=56, fill=255)
bg.putalpha(mask)
bg.save(DOCS / "hero.png", optimize=True)
print(f"wrote {DOCS / 'hero.png'} {bg.size}")
