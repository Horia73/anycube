#!/usr/bin/env python3
"""Makes the application's icon from the project's mark, docs/logo.svg: the
same drawing, set in a square with the margin an application icon has
round it.

    python3 app/icon/make-icon.py        writes icon.svg next to this file
    rsvg-convert -w N -h N icon.svg -o icon-N.png     for N in 16 32 64 128 256 512 1024

The PNG files are kept in the repository, so that installing needs neither
this script nor a converter.
"""
import os
import re

HERE = os.path.dirname(os.path.abspath(__file__))
S = 1024
TILE = 824                 # the mark within the canvas, as macOS icons sit in theirs
M = (S - TILE) / 2

mark = open(os.path.join(HERE, "..", "..", "docs", "logo.svg")).read()
shapes = "\n".join("    " + line for line in re.findall(r"<rect[^>]*/>", mark))
svg = f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {S} {S}">
  <g transform="translate({M:g} {M:g}) scale({TILE / 24:.6g})">
{shapes}
  </g>
</svg>
'''
open(os.path.join(HERE, "icon.svg"), "w").write(svg)
