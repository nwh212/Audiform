// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Built-in presets for the Dynamics Curve effect. Each is written the way a compressor's
 * segment table reads (a ratio and a kind per stretch of the level range), then turned into
 * the curve's points. Named for the job, never for any other product's presets.
 */

import type { EffectPreset } from "./effect-presets";

/** Compressor, expander, neutral (no change), or an expander so steep it closes the sound off. */
export type SegKind = "c" | "e" | "n" | "x";
/** One stretch of the curve: its kind, its ratio and the level (dB) it starts at. */
export type Seg = readonly [SegKind, number, number];

/** How steeply "closed off" falls, in dB out per dB in. */
const CLOSED_SLOPE = 40;
const FLOOR = -120;
const CEILING = 12;

const slopeOf = ([kind, ratio]: Seg): number => (kind === "n" ? 1 : kind === "c" ? 1 / Math.max(ratio, 1) : kind === "e" ? Math.max(ratio, 1) : CLOSED_SLOPE);

/**
 * The curve's points, lowest input first, from a segment table. The table is read top to
 * bottom: the first stretch is everything above its level, and each later stretch runs down
 * from the level it names to the next one. The curve passes through the diagonal (no change)
 * at the first level, so makeup gain is set apart, with the makeup control.
 */
export function curvePoints(segs: readonly Seg[]): [number, number][] {
  if (segs.length < 2) return [[-60, -60], [0, 0]];
  const edges = segs.slice(1).map((s) => s[2]);
  const out: number[] = [edges[0]!];
  for (let k = 1; k < edges.length; k++) out.push(out[k - 1]! - (edges[k - 1]! - edges[k]!) * slopeOf(segs[k]!));
  const clamp = (v: number) => Math.min(CEILING, Math.max(FLOOR, v));
  const pts: [number, number][] = edges.map((x, k) => [x, clamp(out[k]!)]);
  // The top end, at 0 dB in.
  pts.push([0, clamp(out[0]! + (0 - edges[0]!) * slopeOf(segs[0]!))]);
  // The bottom end carries the last stretch's slope past the lowest edge.
  const last = segs[segs.length - 1]!;
  const edge = edges[edges.length - 1]!;
  const base = out[out.length - 1]!;
  if (last[0] === "x") pts.push([edge - 1, clamp(base - CLOSED_SLOPE)]);
  else if (edge > -100) pts.push([-100, clamp(base - (edge + 100) * slopeOf(last))]);
  return pts.sort((a, b) => a[0] - b[0]).map(([x, y]) => [Math.round(x * 100) / 100, Math.round(y * 100) / 100]);
}

/** A curve preset: the segment table, the makeup gain, and a knee in dB (0 for sharp corners). */
export function curvePreset(name: string, makeup: number, segs: readonly Seg[], knee = 0): EffectPreset {
  const pts = curvePoints(segs);
  const params: Record<string, number> = {
    points: pts.length,
    knee,
    makeup,
    detect_attack: 1,
    detect_release: 300,
    rms: 1,
    gain_attack: 0.1,
    gain_release: 150,
    lookahead: 3,
  };
  pts.forEach(([x, y], i) => {
    params[`p${i + 1}_in`] = x;
    params[`p${i + 1}_out`] = y;
  });
  return { name, params };
}

export const DYNAMICS_CURVE_PRESETS: readonly EffectPreset[] = [
  // Voices.
  curvePreset("Narration Curve", 0, [["c", 2.5, -15], ["c", 2, -15]]),
  curvePreset("Vocal, Smooth Curve", 0, [["c", 2, -20], ["n", 1, -20]]),
  curvePreset("Vocal, Driven Curve", 2, [["c", 3.5, -24], ["n", 1, -24]]),
  curvePreset("Vocal, Presence Lift", 0, [["c", 3, -20], ["e", 1.5, -20], ["x", 1, -73.3]]),
  curvePreset("Vocal, Peak Guard Curve", 0, [["c", 6.15, -8], ["n", 1, -8]]),
  curvePreset("Podcast Host Curve", 0, [["c", 3.5, -18.9], ["e", 1.2, -18.9], ["c", 1.3, -27.8], ["e", 2.7, -44.1], ["x", 1, -68.9]], 4),
  // Sibilance.
  curvePreset("Sibilance Curve, Light", 0, [["c", 1.5, -24], ["n", 1, -24]]),
  curvePreset("Sibilance Curve, Medium", 0, [["c", 2, -30], ["n", 1, -30]]),
  curvePreset("Sibilance Curve, Hard", 0, [["c", 4, -35], ["n", 1, -35]]),
  // Ceilings.
  curvePreset("Ceiling Curve, -12 dB", 0, [["c", 20, -12], ["n", 1, -12]]),
  curvePreset("Ceiling Curve, -24 dB", 0, [["c", 20, -24], ["n", 1, -24]], 4),
  curvePreset("Broadcast Ceiling Curve", 0, [["c", 20, -30], ["n", 1, -30]]),
  // Gates and expanders.
  curvePreset("Gate Curve, -10 dB", 0, [["n", 1, -10], ["e", 8.9, -10], ["c", 18.7, -19.6]]),
  curvePreset("Gate Curve, -20 dB", 0, [["n", 1, -20], ["e", 7.1, -20], ["c", 19, -30.8], ["x", 1, -93.7]]),
  curvePreset("Expand Below -10 dB, Tail Lift", 0, [["n", 1, -10], ["e", 3, -10], ["c", 15.3, -38.8], ["x", 1, -93.9]]),
  // Whole-mix shaping.
  curvePreset("Gradual Squeeze", 0, [["c", 5, -10], ["c", 2.5, -10], ["c", 1.5, -24], ["e", 1.2, -40]]),
  curvePreset("Compander, Gentle", 0, [["c", 3.75, -9], ["c", 1.1, -9], ["e", 2, -40], ["x", 1, -71.6]]),
  curvePreset("Mix Bus, Glue Curve", 2, [["c", 2, -10], ["n", 1, -10]]),
  curvePreset("Pump, Maximum", 20, [["c", 21.9, -27.6], ["c", 2.4, -27.6], ["e", 3, -76]]),
];
