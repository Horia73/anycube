#!/usr/bin/env python3
"""Draws the icon: a 3x3x3 cube seen from a corner, on a dark tile.

    python3 app/icon/make-icon.py        writes icon.svg next to this file
    rsvg-convert -w N -h N icon.svg -o icon-N.png     for N in 16 32 64 128 256 512 1024

The PNG files are kept in the repository, so that installing needs neither
this script nor a converter.
"""
import math, os

S = 1024
TILE = 824            # the tile within the canvas, as macOS icons have it
R = 186               # its corner radius
CELL = 104            # one sticker's side along a face
GAP = 0.135           # of a cell, between sticker and cell edge
COS, SIN = math.cos(math.radians(30)), math.sin(math.radians(30))
U = (COS * CELL, SIN * CELL)       # along the top face, to the right and down
V = (-COS * CELL, SIN * CELL)      # along the top face, to the left and down
W = (0.0, CELL)                    # straight down
TOP = (S / 2, S / 2 - 3 * CELL + 6)  # the cube's upper corner

WHITE, GREEN, RED = "#f4f4ef", "#1faa59", "#d92b3a"

def at(origin, a, b, i, j):
    return (origin[0] + a[0] * i + b[0] * j, origin[1] + a[1] * i + b[1] * j)

def face(origin, a, b, colour, shade):
    out = []
    for i in range(3):
        for j in range(3):
            pts = [at(origin, a, b, i + x, j + y) for x, y in ((GAP, GAP), (1 - GAP, GAP), (1 - GAP, 1 - GAP), (GAP, 1 - GAP))]
            d = " ".join(f"{x:.1f},{y:.1f}" for x, y in pts)
            out.append(f'<polygon points="{d}" fill="{colour}" stroke="{colour}" stroke-width="14" stroke-linejoin="round"/>')
    if shade:
        pts = [at(origin, a, b, x, y) for x, y in ((0, 0), (3, 0), (3, 3), (0, 3))]
        d = " ".join(f"{x:.1f},{y:.1f}" for x, y in pts)
        out.append(f'<polygon points="{d}" fill="#000" opacity="{shade}"/>')
    return "\n    ".join(out)

left = at(TOP, U, V, 0, 3)     # the corner on the left
front = at(TOP, U, V, 3, 3)    # the corner towards the eye
right = at(TOP, U, V, 3, 0)
body = [TOP, right, at(right, W, W, 3, 0), at(front, W, W, 3, 0), at(left, W, W, 3, 0), left]
body_d = " ".join(f"{x:.1f},{y:.1f}" for x, y in body)
m = (S - TILE) / 2
svg = f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {S} {S}">
  <defs>
    <linearGradient id="tile" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#262b36"/>
      <stop offset="1" stop-color="#0c0e12"/>
    </linearGradient>
  </defs>
  <rect x="{m}" y="{m}" width="{TILE}" height="{TILE}" rx="{R}" fill="url(#tile)"/>
  <rect x="{m + 3}" y="{m + 3}" width="{TILE - 6}" height="{TILE - 6}" rx="{R - 3}" fill="none" stroke="#fff" stroke-opacity="0.07" stroke-width="6"/>
  <g>
    <polygon points="{body_d}" fill="#07080a" stroke="#07080a" stroke-width="26" stroke-linejoin="round"/>
    {face(TOP, U, V, WHITE, 0)}
    {face(left, U, W, GREEN, 0.10)}
    {face(front, (-V[0], -V[1]), W, RED, 0.22)}
  </g>
</svg>
'''
open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "icon.svg"), "w").write(svg)
