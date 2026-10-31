# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (c) The Audiform contributors.
# This file is part of Audiform, based on Altered Audio Editor.

"""Adds the clip-edge trim pointers ("trim-left" and "trim-right") to every
built-in theme that draws its own pointers, in that theme's style: a short
double arrow beside a bracket on the clip's side, 16 by 16 pixels with the
hot spot in the middle (the theme pointer rule). Run from the repository
root: python3 scripts/themes/make-trim-pointers.py
"""

import base64
import glob
import json

GLASS = """<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 32 32">
  <!-- Audiform scene themes: a glowing blue glass clip-edge trim pointer, a double arrow beside a bracket on the clip's side. Center (16, 16). -->
  <defs>
    <linearGradient id="body" x1="0" y1="0" x2="0.4" y2="1">
      <stop offset="0" stop-color="#d8f8ff"/>
      <stop offset="0.35" stop-color="#5cc8ff"/>
      <stop offset="0.75" stop-color="#1a86f0"/>
      <stop offset="1" stop-color="#0a4fc8"/>
    </linearGradient>
    <filter id="glow" x="-30%" y="-60%" width="160%" height="220%">
      <feGaussianBlur stdDeviation="1.1"/>
    </filter>
  </defs>
  <g transform="{flip}">
    <path d="{shape}" fill="none" stroke="#5ec6ff" stroke-width="2.6" stroke-linejoin="round" filter="url(#glow)"/>
    <path d="{shape}" fill="url(#body)" stroke="#8fdcff" stroke-width="1" stroke-linejoin="round"/>
    <path d="M5 15.2 L8.2 12 M10 14.6 H14 M25.6 10 V22" stroke="#ffffff" stroke-width="0.9" stroke-linecap="round" opacity="0.85"/>
  </g>
</svg>
"""
GLASS_SHAPE = "M2.5 16 L8.5 10 V13.8 H14.5 V10 L20.5 16 L14.5 22 V18.2 H8.5 V22 Z M23 6 H29.5 V26 H23 V22.6 H25.8 V9.4 H23 Z"

MALL = """<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24">
  <!-- Audiform Mall Pastel '95 theme pointer: clip-edge trim. -->
  <g transform="{flip}">{layers}</g>
</svg>
"""
MALL_PATH = "M2 12h12M2 12l3-3M2 12l3 3M14 12l-3-3M14 12l-3 3M17.5 5h3v14h-3"
MALL_LAYERS = [("#5d4b86", 5), ("#ffffff", 3.4), ("#7458d6", 1.9)]

# Phosphor: square pixels, two screen pixels each, on an 8 by 8 grid.
PHOS_FILL = "#7dff9c"
PHOS_EDGE = "#021004"
PHOS_PIXELS = {
    # The double arrow: a shaft with a head at each end.
    (1, 2), (0, 3), (1, 3), (2, 3), (3, 3), (4, 3), (1, 4), (3, 2), (3, 4),
    # The bracket on the clip's side.
    (6, 1), (6, 2), (6, 3), (6, 4), (6, 5), (5, 1), (5, 5),
}


def glass(right: bool) -> str:
    flip = "translate(32 0) scale(-1 1)" if right else ""
    return GLASS.replace("{flip}", flip).replace("{shape}", GLASS_SHAPE)


def mall(right: bool) -> str:
    flip = "translate(24 0) scale(-1 1)" if right else ""
    layers = "".join(
        f'<g fill="none" stroke="{c}" stroke-width="{w}" stroke-linecap="round" stroke-linejoin="round"><path d="{MALL_PATH}"/></g>'
        for c, w in MALL_LAYERS
    )
    return MALL.replace("{flip}", flip).replace("{layers}", layers)


def phosphor(right: bool) -> str:
    fill = {(7 - x, y) if right else (x, y) for x, y in PHOS_PIXELS}
    edge = set()
    for x, y in fill:
        for dx in (-1, 0, 1):
            for dy in (-1, 0, 1):
                p = (x + dx, y + dy)
                if p not in fill and 0 <= p[0] < 8 and 0 <= p[1] < 8:
                    edge.add(p)
    rects = lambda ps, c: f'<g fill="{c}">' + "".join(
        f'<rect x="{x}" y="{y}" width="1" height="1"/>' for x, y in sorted(ps)
    ) + "</g>"
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 8 8" shape-rendering="crispEdges">\n'
        "  <!-- Audiform Phosphor theme: a clip-edge trim pointer, square pixels (2 screen pixels each) in phosphor green. Hot spot (8, 8). -->\n  "
        + rects(edge, PHOS_EDGE)
        + rects(fill, PHOS_FILL)
        + "\n</svg>\n"
    )


STYLES = {
    "aqua": glass,
    "meadow": glass,
    "moonlight": glass,
    "prism": glass,
    "mallpastel": mall,
    "mallnight": mall,
    "phosphor": phosphor,
}


def data(svg: str) -> str:
    return "data:image/svg+xml;base64," + base64.b64encode(svg.encode()).decode()


for path in sorted(glob.glob("src/themes/*.audtheme")):
    with open(path) as f:
        theme = json.load(f)
    make = STYLES.get(theme.get("id", ""))
    if not make or not theme.get("pointers"):
        continue
    for side in ("left", "right"):
        name = f"cursor-trim-{side}"
        theme["assets"][name] = data(make(side == "right"))
        theme["pointers"][f"trim-{side}"] = {"asset": name, "hot": [8, 8]}
    with open(path, "w") as f:
        json.dump(theme, f, indent=2, ensure_ascii=False)
        f.write("\n")
    print("trim pointers:", path)
