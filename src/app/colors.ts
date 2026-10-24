// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

/**
 * Color ramps for the analysis views (the spectrogram and the Phase
 * Wheel): two colors the user picks, blended around the color wheel so a
 * red-to-magenta pair passes through every hue in between.
 */

export type Rgb = [number, number, number];

export function hexToRgb(hex: string): Rgb | null {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return null;
  const n = parseInt(m[1]!, 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

export function rgbToHex([r, g, b]: Rgb): string {
  return `#${[r, g, b].map((v) => Math.round(Math.max(0, Math.min(255, v))).toString(16).padStart(2, "0")).join("")}`;
}

/** Hue 0 to 360, saturation and lightness 0 to 1. */
export function rgbToHsl([r, g, b]: Rgb): [number, number, number] {
  const [R, G, B] = [r / 255, g / 255, b / 255];
  const max = Math.max(R, G, B);
  const min = Math.min(R, G, B);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  const h = max === R ? (G - B) / d + (G < B ? 6 : 0) : max === G ? (B - R) / d + 2 : (R - G) / d + 4;
  return [h * 60, s, l];
}

export function hslToRgb(h: number, s: number, l: number): Rgb {
  const hue = (((h % 360) + 360) % 360) / 360;
  if (s === 0) return [l * 255, l * 255, l * 255];
  const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
  const p = 2 * l - q;
  const f = (t: number) => {
    const u = ((t % 1) + 1) % 1;
    if (u < 1 / 6) return p + (q - p) * 6 * u;
    if (u < 1 / 2) return q;
    if (u < 2 / 3) return p + (q - p) * (2 / 3 - u) * 6;
    return p;
  };
  return [f(hue + 1 / 3) * 255, f(hue) * 255, f(hue - 1 / 3) * 255];
}

/**
 * The color `t` (0 to 1) of the way from `low` to `high`, going up the
 * color wheel from low's hue to high's (so red to magenta passes orange,
 * yellow, green, blue), with saturation and lightness blended.
 */
export function ramp(low: Rgb, high: Rgb, t: number): Rgb {
  const [h0, s0, l0] = rgbToHsl(low);
  const [h1raw, s1, l1] = rgbToHsl(high);
  const h1 = h1raw < h0 ? h1raw + 360 : h1raw;
  const u = Math.max(0, Math.min(1, t));
  return hslToRgb(h0 + (h1 - h0) * u, s0 + (s1 - s0) * u, l0 + (l1 - l0) * u);
}

/** Defaults: the Phase Wheel runs red (low notes) to magenta (high). */
export const PHASE_LOW = "#ff2a2a";
export const PHASE_HIGH = "#e02aff";
/** Defaults for a custom spectrogram: deep blue (quiet) to warm yellow (loud). */
export const SPECTRUM_LOW = "#2a1a8a";
export const SPECTRUM_HIGH = "#ffe066";

/**
 * A 256-entry spectrogram palette (RGB triples, flattened): near black
 * for silence, fading into `low` for quiet sound and up the color wheel
 * to `high` for the loudest.
 */
export function spectrumLut(lowHex: string, highHex: string): Uint8Array {
  const low = hexToRgb(lowHex) ?? hexToRgb(SPECTRUM_LOW)!;
  const high = hexToRgb(highHex) ?? hexToRgb(SPECTRUM_HIGH)!;
  const black: Rgb = [8, 8, 13];
  const out = new Uint8Array(256 * 3);
  for (let i = 0; i < 256; i++) {
    const t = i / 255;
    const c: Rgb =
      t < 0.25
        ? [0, 1, 2].map((j) => black[j]! + (low[j]! - black[j]!) * (t / 0.25)) as Rgb
        : ramp(low, high, (t - 0.25) / 0.75);
    for (let j = 0; j < 3; j++) out[i * 3 + j] = Math.round(Math.max(0, Math.min(255, c[j]!)));
  }
  return out;
}
