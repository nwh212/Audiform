# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (c) The Audiform contributors.
# This file is part of Audiform, based on Altered Audio Editor.

"""
Makes the blurred copies of the four glass themes' scenes.

The glass themes (Meadow, Aqua, Moonlight, Prism) used `backdrop-filter:
blur(...)` on every panel. The scene behind never changes, but the
browser blurs it again whenever anything inside a panel repaints (a moving
playhead, the Phase Wheel, meters), which cost most of the frame time.

Each panel now paints a copy of the scene that was blurred once, here,
fixed to the window like the scene itself, so it looks the same and costs
nothing per frame. The copies are small (a blur has no detail to keep) and
are stretched to cover the window like the scene is.

The blur, saturation and brightness below are the ones the old CSS used.
The blur is in window pixels at a 1920 pixel wide window, so a copy 640
pixels wide uses a third of it. Run from the repository root:

    python3 scripts/themes/make-glass-blur.py
"""

import sys

import numpy as np
from PIL import Image, ImageFilter

SRC = "src/assets/themes"
WIDTH = int(sys.argv[1]) if len(sys.argv) > 1 else 640
REFERENCE_WINDOW = 1920

# name -> (source, blur px, saturate, brightness)
SCENES = {
    "meadow": ("meadow-photo.webp", 26, 1.5, 1.06),
    "aqua": ("aqua.webp", 26, 1.5, 1.06),
    "moonlight": ("moonlight.webp", 26, 1.4, 0.85),
    "prism": ("prism.webp", 26, 1.4, 0.85),
}


def saturate(a: np.ndarray, s: float) -> np.ndarray:
    """The CSS saturate() matrix (Rec. 709 weights), on 0..1 RGB."""
    m = np.array(
        [
            [0.213 + 0.787 * s, 0.715 - 0.715 * s, 0.072 - 0.072 * s],
            [0.213 - 0.213 * s, 0.715 + 0.285 * s, 0.072 - 0.072 * s],
            [0.213 - 0.213 * s, 0.715 - 0.715 * s, 0.072 + 0.928 * s],
        ]
    )
    return np.clip(a @ m.T, 0, 1)


for name, (source, blur, sat, bright) in SCENES.items():
    im = Image.open(f"{SRC}/{source}").convert("RGB")
    h = round(im.height * WIDTH / im.width)
    small = im.resize((WIDTH, h), Image.LANCZOS)
    small = small.filter(ImageFilter.GaussianBlur(blur * WIDTH / REFERENCE_WINDOW))
    a = np.asarray(small, dtype=np.float64) / 255
    a = np.clip(saturate(a, sat) * bright, 0, 1)
    out = Image.fromarray((a * 255 + 0.5).astype(np.uint8))
    out.save(f"{SRC}/{name}-glass.webp", quality=92, method=6)
    print(name, out.size)
