#!/usr/bin/env python3
"""Draws the application's icon: the project's mark (docs/logo.svg, nine
stickers on a dark tile) set as an application icon, with the margin round
the tile that such icons have.

    python3 app/icon/make-icon.py        writes icon.svg next to this file
    rsvg-convert -w N -h N icon.svg -o icon-N.png     for N in 16 32 64 128 256 512 1024

The PNG files are kept in the repository, so that installing needs neither
this script nor a converter.
"""
import os

S = 1024
TILE = 824                 # the tile within the canvas, as macOS icons have it
K = TILE / 24              # the mark is drawn on a grid of 24
M = (S - TILE) / 2
COLOURS = [
    ["#f4f4ef", "#d92b3a", "#1faa59"],
    ["#f7d038", "#f58220", "#2f6fde"],
    ["#d92b3a", "#2f6fde", "#f7d038"],
]

stickers = []
for row in range(3):
    for col in range(3):
        x, y = M + (3 + 6.4 * col) * K, M + (3 + 6.4 * row) * K
        stickers.append(f'<rect x="{x:.1f}" y="{y:.1f}" width="{5.2 * K:.1f}" height="{5.2 * K:.1f}" rx="{1.2 * K:.1f}" fill="{COLOURS[row][col]}"/>')

svg = f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {S} {S}">
  <defs>
    <linearGradient id="tile" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#1c2029"/>
      <stop offset="1" stop-color="#0b0d10"/>
    </linearGradient>
  </defs>
  <rect x="{M}" y="{M}" width="{TILE}" height="{TILE}" rx="{5 * K:.1f}" fill="url(#tile)"/>
  <rect x="{M + 3}" y="{M + 3}" width="{TILE - 6}" height="{TILE - 6}" rx="{5 * K - 3:.1f}" fill="none" stroke="#fff" stroke-opacity="0.07" stroke-width="6"/>
  {chr(10).join("  " + s for s in stickers).strip()}
</svg>
'''
open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "icon.svg"), "w").write(svg)
