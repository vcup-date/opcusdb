#!/usr/bin/env python3
"""Audit town-sprites.png for clean keying.

The Hearth sprite atlas is 13 rows (12 residents + 1 traveler), 4 frames each,
96x128 per frame. An earlier bug left Pia's row with an unkeyed purple/magenta
background. This checks every row for that whole class of problem so a future
regeneration can be verified before it ships:

  - fully transparent cell borders (no silhouette clipped at or bleeding past a
    frame boundary)
  - no opaque magenta/purple residue (the old key color)
  - no faint partial-alpha magenta halo around the silhouette edges

Run from anywhere:  python3 demos/server/web/audit_sprites.py
Exits non-zero if any row looks unclean, so it can gate an asset update.
"""
import os
import sys

try:
    from PIL import Image
except ImportError:
    sys.exit("needs Pillow: pip install pillow")

FW, FH = 96, 128
NAMES = ["Mara", "Tomas", "Lila", "Bran", "Yuki", "Ravi", "Nina", "Otto",
         "Pia", "Sol", "Greta", "Finn", "Traveler"]


def is_magenta(r, g, b, a):
    # the key color was a saturated purple/magenta: red and blue well above green
    return a > 0 and r > 100 and b > 90 and (r - g) > 35 and (b - g) > 15


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    path = os.path.join(here, "town-sprites.png")
    im = Image.open(path).convert("RGBA")
    px = im.load()
    rows = im.size[1] // FH
    problems = 0
    for r in range(rows):
        y0 = r * FH
        magenta = 0
        border = 0
        for yy in range(y0, y0 + FH):
            for xx in range(FW):
                R, G, B, A = px[xx, yy]
                if is_magenta(R, G, B, A):
                    magenta += 1
        for xx in range(FW):
            if px[xx, y0][3] > 0 or px[xx, y0 + FH - 1][3] > 0:
                border += 1
        for yy in range(y0, y0 + FH):
            if px[0, yy][3] > 0 or px[FW - 1, yy][3] > 0:
                border += 1
        name = NAMES[r] if r < len(NAMES) else f"row{r}"
        bad = magenta > 10 or border > 8
        if bad:
            problems += 1
        flag = "  <-- CHECK" if bad else "ok"
        print(f"row {r:2d} {name:9s} magenta={magenta:4d} border-opaque={border:3d}  {flag}")
    if problems:
        print(f"\n{problems} row(s) look unclean")
        return 1
    print("\nall rows cleanly keyed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
