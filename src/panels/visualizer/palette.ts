// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { hexToRgb, hslToRgb, ramp, type Rgb } from "../../app/colors";

export type Scheme = "theme" | "rainbow" | "fire" | "ocean" | "aurora";

export const SCHEMES: { id: Scheme; label: string }[] = [
  { id: "theme", label: "Theme colors" },
  { id: "rainbow", label: "Rainbow" },
  { id: "fire", label: "Fire" },
  { id: "ocean", label: "Ocean" },
  { id: "aurora", label: "Aurora" },
];

/** A color for a position t from 0 to 1 (it wraps, so drifting t is fine). */
export type Palette = (t: number) => Rgb;

function wrap(t: number): number {
  return ((t % 1) + 1) % 1;
}

function stops(list: Rgb[]): Palette {
  return (t) => {
    const x = wrap(t) * (list.length - 1);
    const i = Math.min(list.length - 2, Math.floor(x));
    const f = x - i;
    const a = list[i]!;
    const b = list[i + 1]!;
    return [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f];
  };
}

/** `low` and `high` are the theme's own two analysis colors (css colors, or null for the built-in pair). */
export function paletteFor(scheme: Scheme, low: string | null, high: string | null): Palette {
  switch (scheme) {
    case "rainbow":
      return (t) => hslToRgb(wrap(t) * 360, 0.85, 0.55);
    case "fire":
      return stops([[40, 0, 10], [190, 20, 20], [255, 110, 20], [255, 210, 70], [255, 245, 200]]);
    case "ocean":
      return stops([[4, 12, 60], [18, 70, 160], [40, 170, 200], [150, 230, 225], [235, 250, 245]]);
    case "aurora":
      return stops([[10, 30, 70], [20, 150, 110], [120, 230, 120], [170, 110, 230], [20, 150, 110]]);
    default: {
      const a = (low && hexToRgb(low)) || ([255, 80, 80] as Rgb);
      const b = (high && hexToRgb(high)) || ([230, 80, 255] as Rgb);
      return (t) => ramp(a, b, 0.5 - 0.5 * Math.cos(wrap(t) * Math.PI * 2));
    }
  }
}

export function css([r, g, b]: Rgb, alpha = 1): string {
  return `rgba(${Math.round(r)},${Math.round(g)},${Math.round(b)},${alpha})`;
}
