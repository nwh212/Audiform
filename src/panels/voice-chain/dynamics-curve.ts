// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Dynamics Curve's points and curve for its graph editor. The curve must match
 * `src-tauri/src/dsp/effects/curve_dynamics.rs` (`points_from`, `curve_linear` and `build_table`).
 */

import type { EffectSlotDoc } from "../../stores/session";

export const DYNAMICS_CURVE = "altaudio.dynamics_curve";
export const MAX_POINTS = 8;
export const inKey = (i: number) => `p${i + 1}_in`;
export const outKey = (i: number) => `p${i + 1}_out`;

/** Where each point starts, matching the core's: a straight line, so a new effect changes nothing. */
const START = [-60, 0, -40, -30, -20, -10, -5, -80];
/** Dips of the graph, dB. Controls reach lower than the graph draws. */
export const IN_MIN = -100;
export const IN_MAX = 0;
export const OUT_MIN = -100;
export const OUT_MAX = 12;
/** dB between samples of the core's lookup table. */
const STEP = 0.25;

export interface CurvePoint {
  /** Which point (0 to 7): its controls are `p{index + 1}_in` and `_out`. */
  index: number;
  inDb: number;
  outDb: number;
}

const get = (slot: EffectSlotDoc, key: string, def: number) => slot.params[key] ?? def;

export function pointCount(slot: EffectSlotDoc): number {
  return Math.min(MAX_POINTS, Math.max(2, Math.round(get(slot, "points", 2))));
}

/** The points in use, lowest input first. */
export function curvePoints(slot: EffectSlotDoc): CurvePoint[] {
  return Array.from({ length: pointCount(slot) }, (_, i) => ({
    index: i,
    inDb: get(slot, inKey(i), START[i]!),
    outDb: get(slot, outKey(i), START[i]!),
  })).sort((a, b) => a.inDb - b.inDb);
}

/** Straight between points, carrying on past the ends at the slope of the nearest segment. */
export function curveLinear(pts: readonly CurvePoint[], x: number): number {
  let y: number;
  if (pts.length === 0) y = x;
  else if (pts.length === 1) y = pts[0]!.outDb + (x - pts[0]!.inDb);
  else {
    const seg = (a: CurvePoint, b: CurvePoint) => (b.inDb - a.inDb === 0 ? 1 : (b.outDb - a.outDb) / (b.inDb - a.inDb));
    const first = pts[0]!;
    const last = pts[pts.length - 1]!;
    if (x <= first.inDb) y = first.outDb + (x - first.inDb) * seg(first, pts[1]!);
    else if (x >= last.inDb) y = last.outDb + (x - last.inDb) * seg(pts[pts.length - 2]!, last);
    else {
      let i = 0;
      for (let k = 0; k < pts.length; k++) if (pts[k]!.inDb <= x) i = k;
      i = Math.min(i, pts.length - 2);
      y = pts[i]!.outDb + (x - pts[i]!.inDb) * seg(pts[i]!, pts[i + 1]!);
    }
  }
  return Math.max(-140, Math.min(60, y));
}

/** The curve with corners rounded by averaging over a window `knee` dB wide, as the core does. */
export function curveAt(pts: readonly CurvePoint[], knee: number, x: number): number {
  const half = Math.round(knee / STEP / 2);
  if (half === 0) return curveLinear(pts, x);
  let sum = 0;
  for (let j = -half; j <= half; j++) sum += curveLinear(pts, x + j * STEP);
  return sum / (2 * half + 1);
}

/** The control changes that add a point (the next unused one) at `inDb`, `outDb`. Null when all eight are in use. */
export function addPoint(slot: EffectSlotDoc, inDb: number, outDb: number): Record<string, number> | null {
  const n = pointCount(slot);
  if (n >= MAX_POINTS) return null;
  return { points: n + 1, [inKey(n)]: inDb, [outKey(n)]: outDb };
}

/** The changes that remove point `index`: the last point in use takes its place. Null below three points. */
export function removePoint(slot: EffectSlotDoc, index: number): Record<string, number> | null {
  const n = pointCount(slot);
  if (n <= 2 || index >= n) return null;
  const last = n - 1;
  const out: Record<string, number> = { points: n - 1 };
  if (index !== last) {
    out[inKey(index)] = get(slot, inKey(last), START[last]!);
    out[outKey(index)] = get(slot, outKey(last), START[last]!);
  }
  return out;
}

/** The changes for a flat line (no change to the sound): two points on the diagonal. */
export const RESET: Record<string, number> = { points: 2, p1_in: -60, p1_out: -60, p2_in: 0, p2_out: 0, knee: 0 };

/** Whether a group of controls shows below the graph: unused points stay out of the way. */
export function curveGroupShown(effect: string, slot: EffectSlotDoc, group: string): boolean {
  if (effect !== DYNAMICS_CURVE) return true;
  const m = /^Point (\d+)$/.exec(group);
  return !m || Number(m[1]) <= pointCount(slot);
}
