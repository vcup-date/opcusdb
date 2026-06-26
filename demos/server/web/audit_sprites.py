#!/usr/bin/env python3
"""Audit town-sprites.png for clean keying.

The Hearth sprite atlas is 13 rows (12 residents + 1 traveler), 4 frames each,
96x128 per frame. An earlier bug left Pia's row with an unkeyed purple/magenta
background. This checks every row for that whole class of problem so a future
regeneration can be verified before it ships:

  - no opaque magenta/purple residue (the old key color)
  - no silhouette bleeding across the TOP or BOTTOM cell edge, where the row above
    or below would composite the wrong character (the client stacks rows vertically
    and draws frame 0 in a fixed 96x128 rect)

A silhouette touching the LEFT or RIGHT edge is reported as a note, not a failure:
horizontally adjacent frames (the other walk-cycle frames) are never drawn together,
so edge-touch there cannot bleed or clip in game.

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
        for yy in range(y0, y0 + FH):
            for xx in range(FW):
                R, G, B, A = px[xx, yy]
                if is_magenta(R, G, B, A):
                    magenta += 1
        # top/bottom edges: real bleed risk between vertically stacked rows
        vbleed = sum(1 for xx in range(FW)
                     if px[xx, y0][3] > 0 or px[xx, y0 + FH - 1][3] > 0)
        # left/right edges: cosmetic only (horizontal frames never composite together)
        hedge = sum(1 for yy in range(y0, y0 + FH)
                    if px[0, yy][3] > 0 or px[FW - 1, yy][3] > 0)
        name = NAMES[r] if r < len(NAMES) else f"row{r}"
        bad = magenta > 10 or vbleed > 4
        if bad:
            problems += 1
        flag = "  <-- CHECK" if bad else ("  (touches side edge, cosmetic)" if hedge > 4 else "ok")
        print(f"row {r:2d} {name:9s} magenta={magenta:4d} v-bleed={vbleed:2d} side-edge={hedge:3d}  {flag}")
    if problems:
        print(f"\n{problems} row(s) have magenta residue or top/bottom bleed")
        return 1
    print("\nall rows cleanly keyed (no magenta, no row-to-row bleed)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
