#!/usr/bin/env python3
"""Add text labels to a rendered sheet (the engine has no text yet).

Usage: python3 -I tools/label_sheet.py <sheet.png> [scale]
Reads <sheet.png>.labels.tsv (x, y, size, text), draws the labels with Inter (falls back to
Pillow's default font), optionally downscales, and overwrites the PNG. Removes the TSV.
"""
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

FONT_DIRS = ["/usr/share/fonts/opentype/inter", "/usr/share/fonts/truetype/inter", "assets/fonts"]


def font(size, bold):
    for d in FONT_DIRS:
        f = Path(d) / ("Inter-SemiBold.otf" if bold else "Inter-Regular.otf")
        if f.exists():
            return ImageFont.truetype(str(f), size)
    return ImageFont.load_default()


def main():
    png = Path(sys.argv[1])
    scale = float(sys.argv[2]) if len(sys.argv) > 2 else 1.0
    tsv = Path(str(png) + ".labels.tsv")
    img = Image.open(png).convert("RGB")
    draw = ImageDraw.Draw(img)
    for line in tsv.read_text().splitlines():
        x, y, size, text = line.split("\t", 3)
        size = int(size)
        draw.text((int(x), int(y)), text, fill=(40, 40, 46), font=font(size, size >= 30))
    if scale != 1.0:
        img = img.resize((round(img.width * scale), round(img.height * scale)), Image.LANCZOS)
    img.save(png, optimize=True)
    tsv.unlink()
    print(f"labelled {png} ({img.width} x {img.height})")


if __name__ == "__main__":
    main()
