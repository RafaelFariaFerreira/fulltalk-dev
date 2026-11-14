#!/usr/bin/env python3
"""Generates the Windows and macOS app icons from the geometry of docs/logo.svg.

    packaging/windows/fulltalk.ico   16-256 px, used by the Start Menu shortcut
    packaging/macos/fulltalk.icns    16-1024 px, used by the .app bundle

Linux installs docs/logo.svg directly. Requires Pillow.
"""
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
VIOLET = (0x5B, 0x3F, 0xE4, 255)
WHITE = (255, 255, 255, 255)
# docs/logo.svg, in its 128-unit viewBox: (x, y, width, height) of each bar.
BARS = [
    (21.4, 46.1, 10.9, 35.8),
    (40.0, 30.7, 10.9, 66.6),
    (58.6, 14.1, 10.9, 99.8),
    (77.2, 30.7, 10.9, 66.6),
    (95.8, 46.1, 10.9, 35.8),
]
CORNER = 30


def logo(size: int, margin: float = 0.0) -> Image.Image:
    """The logo at `size` px, inset by `margin` (a fraction of the canvas)."""
    ss = 4  # supersampling
    canvas = size * ss
    inset = canvas * margin
    unit = (canvas - 2 * inset) / 128

    def box(x: float, y: float, w: float, h: float) -> list[float]:
        return [inset + x * unit, inset + y * unit, inset + (x + w) * unit, inset + (y + h) * unit]

    image = Image.new("RGBA", (canvas, canvas), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle(box(0, 0, 128, 128), radius=CORNER * unit, fill=VIOLET)
    for x, y, w, h in BARS:
        draw.rounded_rectangle(box(x, y, w, h), radius=w / 2 * unit, fill=WHITE)
    return image.resize((size, size), Image.LANCZOS)


def main() -> None:
    ico = ROOT / "packaging" / "windows" / "fulltalk.ico"
    ico.parent.mkdir(parents=True, exist_ok=True)
    sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    frames = [logo(s) for s in sizes]
    frames[-1].save(ico, sizes=[(s, s) for s in sizes], append_images=frames[:-1])
    print(f"wrote {ico.relative_to(ROOT)}")

    # macOS icons leave ~10% of the canvas empty around the shape, like the system ones.
    icns = ROOT / "packaging" / "macos" / "fulltalk.icns"
    icns.parent.mkdir(parents=True, exist_ok=True)
    logo(1024, margin=0.098).save(icns)
    print(f"wrote {icns.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
