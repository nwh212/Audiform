// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Phase Curve's points and curve for its graph editor. The curve must
 * match `src-tauri/src/dsp/effects/phase_curve.rs` (`curve_at`).
 */

import type { EffectSlotDoc } from "../../stores/session";

export const PHASE_CURVE = "altaudio.phase_curve";
export const MAX_POINTS = 8;
export const freqKey = (i: number) => `p${i + 1}_freq`;
export const phaseKey = (i: number) => `p${i + 1}_phase`;

/** Default spots for each point, matching the core's. */
const SPREAD = [20, 20_000, 100, 500, 1_000, 2_000, 5_000, 10_000];

export interface CurvePoint {
  /** Which point (0 to 7): its controls are `p{index + 1}_freq` and `_phase`. */
  index: number;
  hz: number;
  deg: number;
}

const get = (slot: EffectSlotDoc, key: string, def: number) => slot.params[key] ?? def;

export function pointCount(slot: EffectSlotDoc): number {
  return Math.min(MAX_POINTS, Math.max(2, Math.round(get(slot, "points", 2))));
}

export function rangeOf(slot: EffectSlotDoc): number {
  return Math.round(get(slot, "range", 0)) === 1 ? 360 : 180;
}

/** The points in use, by frequency. */
export function curvePoints(slot: EffectSlotDoc): CurvePoint[] {
  const limit = rangeOf(slot);
  return Array.from({ length: pointCount(slot) }, (_, i) => ({
    index: i,
    hz: Math.max(1, get(slot, freqKey(i), SPREAD[i]!)),
    deg: Math.max(-limit, Math.min(limit, get(slot, phaseKey(i), 0))),
  })).sort((a, b) => a.hz - b.hz);
}

/** The curve's angle at `hz`: straight on a log scale, or a smooth curve through the points. */
export function curveAt(pts: readonly CurvePoint[], spline: boolean, hz: number): number {
  if (pts.length === 0) return 0;
  const first = pts[0]!;
  const last = pts[pts.length - 1]!;
  if (hz <= first.hz) return first.deg;
  if (hz >= last.hz) return last.deg;
  let i = 0;
  for (let k = 0; k < pts.length; k++) if (pts[k]!.hz <= hz) i = k;
  i = Math.min(i, pts.length - 2);
  const x1 = Math.log(pts[i]!.hz);
  const x2 = Math.log(pts[i + 1]!.hz);
  const span = Math.max(1e-12, x2 - x1);
  const t = Math.min(1, Math.max(0, (Math.log(Math.max(1, hz)) - x1) / span));
  const y1 = pts[i]!.deg;
  const y2 = pts[i + 1]!.deg;
  if (!spline || pts.length < 3) return y1 + (y2 - y1) * t;
  const slope = (k: number) => {
    const a = pts[Math.max(0, k - 1)]!;
    const b = pts[Math.min(pts.length - 1, k + 1)]!;
    const dx = Math.log(b.hz) - Math.log(a.hz);
    return dx <= 1e-12 ? 0 : (b.deg - a.deg) / dx;
  };
  const m1 = slope(i) * span;
  const m2 = slope(i + 1) * span;
  const t2 = t * t;
  const t3 = t2 * t;
  return (2 * t3 - 3 * t2 + 1) * y1 + (t3 - 2 * t2 + t) * m1 + (-2 * t3 + 3 * t2) * y2 + (t3 - t2) * m2;
}

/** The control changes that add a point (the next unused one) at `hz`, `deg`. Null when all eight are in use. */
export function addPoint(slot: EffectSlotDoc, hz: number, deg: number): Record<string, number> | null {
  const n = pointCount(slot);
  if (n >= MAX_POINTS) return null;
  return { points: n + 1, [freqKey(n)]: hz, [phaseKey(n)]: deg };
}

/** The changes that remove point `index`: the last point in use takes its place. Null below three points. */
export function removePoint(slot: EffectSlotDoc, index: number): Record<string, number> | null {
  const n = pointCount(slot);
  if (n <= 2 || index >= n) return null;
  const last = n - 1;
  const out: Record<string, number> = { points: n - 1 };
  if (index !== last) {
    out[freqKey(index)] = get(slot, freqKey(last), SPREAD[last]!);
    out[phaseKey(index)] = get(slot, phaseKey(last), 0);
  }
  return out;
}

/** Whether a group of controls shows below the graph: unused points stay out of the way. */
export function groupShown(effect: string, slot: EffectSlotDoc, group: string): boolean {
  if (effect !== PHASE_CURVE) return true;
  const m = /^Point (\d+)$/.exec(group);
  return !m || Number(m[1]) <= pointCount(slot);
}
