#!/usr/bin/env python3
"""Composes docs/hero.png from the app screenshots (requires Pillow).

Generate the screenshots first:
    scripts/screenshots.sh
"""
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

DOCS = Path(__file__).resolve().parent.parent / "docs"
main = Image.open(DOCS / "screenshots" / "toast.png").convert("RGBA")
settings = Image.open(DOCS / "screenshots" / "settings.png").convert("RGBA")

PAD, GAP = 150, 90
W = PAD * 2 + main.width + GAP + settings.width
H = max(main.height, settings.height) + 160

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

y = (H - main.height) // 2
bg.alpha_composite(main, (PAD, y))
bg.alpha_composite(settings, (PAD + main.width + GAP, y + 40))
bg.convert("RGB").save(DOCS / "hero.png", optimize=True)
print(f"wrote {DOCS / 'hero.png'} {bg.size}")
