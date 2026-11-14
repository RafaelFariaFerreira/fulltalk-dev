#!/usr/bin/env python3
"""Renders the README illustrations: docs/illustrations/*.svg -> PNG/WebP at 2x.

The SVGs are the source of truth. They are rasterised with headless Chrome so
the text uses the same Inter files the app ships, whatever fonts the machine
(or GitHub's image proxy) has installed.

    scripts/make-illustrations.py            # all of them
    scripts/make-illustrations.py cover      # just one

Requires Google Chrome or Chromium on PATH (or set CHROME=/path/to/chrome),
plus Pillow for the WebP cover.
"""
import base64
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "docs" / "illustrations"
FONTS = ROOT / "crates" / "fulltalk" / "assets" / "fonts"

# name -> (themes, format). A themed SVG styles itself through `svg.dark { --var: ... }`.
# The cover is a full-bleed gradient: 1.7 MB as a PNG, about 130 kB as a WebP.
JOBS = {
    "cover": ([None], "webp"),
    "how-it-works": (["light", "dark"], "png"),
    "local": (["light", "dark"], "png"),
}


def chrome() -> str:
    candidates = [os.environ.get("CHROME"), "google-chrome", "chromium", "chromium-browser"]
    for name in filter(None, candidates):
        if path := shutil.which(name):
            return path
    sys.exit("Chrome/Chromium not found. Install it or set CHROME=/path/to/chrome.")


def font_face(weight: int, file: str) -> str:
    data = base64.b64encode((FONTS / file).read_bytes()).decode()
    return (
        f"@font-face{{font-family:Inter;font-weight:{weight};"
        f"src:url(data:font/ttf;base64,{data}) format('truetype')}}"
    )


def render(name: str, theme: str | None, fmt: str, browser: str, workdir: Path) -> Path:
    svg = (SRC / f"{name}.svg").read_text(encoding="utf-8")
    width, height = (int(v) for v in re.search(r'width="(\d+)" height="(\d+)"', svg).groups())
    if theme == "dark":
        svg = svg.replace("<svg ", '<svg class="dark" ', 1)

    page = workdir / f"{name}.html"
    page.write_text(
        "<!doctype html><meta charset='utf-8'><style>"
        + font_face(400, "Inter-Regular.ttf")
        + font_face(600, "Inter-SemiBold.ttf")
        + "html,body{margin:0;background:transparent}svg{display:block}</style>"
        + svg,
        encoding="utf-8",
    )
    stem = f"{name}-{theme}" if theme else name
    out = SRC / f"{stem}.{fmt}"
    shot = out if fmt == "png" else workdir / f"{stem}.png"
    subprocess.run(
        [
            browser,
            "--headless=new",
            "--disable-gpu",
            "--hide-scrollbars",
            "--force-device-scale-factor=2",
            "--default-background-color=00000000",
            f"--window-size={width},{height}",
            f"--screenshot={shot}",
            page.as_uri(),
        ],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    if fmt == "webp":
        from PIL import Image

        Image.open(shot).save(out, quality=92, method=6)
    return out


def main() -> None:
    wanted = sys.argv[1:] or list(JOBS)
    if unknown := [n for n in wanted if n not in JOBS]:
        sys.exit(f"unknown illustration: {', '.join(unknown)} (have: {', '.join(JOBS)})")
    browser = chrome()
    with tempfile.TemporaryDirectory() as tmp:
        for name in wanted:
            themes, fmt = JOBS[name]
            for theme in themes:
                out = render(name, theme, fmt, browser, Path(tmp))
                print(f"wrote {out.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
