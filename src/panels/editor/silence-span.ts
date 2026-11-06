// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Silence Time Selection in Selected Clips: a dip in each clip's volume envelope across the
 * selected time, with short ramps in and out so there is no click. The audio is untouched, so
 * the dip can be dragged, reshaped or removed like any other envelope.
 */

import { clipLength, type ClipChange, type ClipDoc, type EnvelopePointDoc, type SessionDoc } from "../../stores/session";
import { DB_MIN, levelAt, pointsOf } from "./envelope";

/** Length of the ramp on each side of the silence, in seconds. */
export const SILENCE_RAMP_SECONDS = 0.005;

/**
 * The volume points of a clip `len` samples long with `from` to `to` (samples from its start)
 * silenced. Points inside the span go, the levels just outside it are kept by anchor points, and the
 * rest of the line is unchanged. Null when the span misses the clip.
 */
export function silenceSpan(points: readonly EnvelopePointDoc[], len: number, from: number, to: number, ramp: number): EnvelopePointDoc[] | null {
  const lo = Math.max(0, Math.round(from));
  const hi = Math.min(len, Math.round(to));
  if (hi <= lo) return null;
  const r = Math.max(0, Math.round(ramp));
  const inL = Math.min(r, lo);
  const inR = Math.min(r, len - hi);
  const pt = (position: number, value: number): EnvelopePointDoc => ({ position, value, curve: "linear" });
  const out: EnvelopePointDoc[] = points.filter((p) => p.position < lo - inL).map((p) => ({ ...p }));
  if (inL > 0) out.push(pt(lo - inL, levelAt(points, lo - inL)));
  out.push(pt(lo, DB_MIN));
  if (hi > lo) out.push(pt(hi, DB_MIN));
  if (inR > 0) out.push(pt(hi + inR, levelAt(points, hi + inR)));
  for (const p of points) if (p.position > hi + inR) out.push({ ...p });
  return out;
}

/** The changes that silence the session range `start` to `end` in the given clips, one per clip it touches. */
export function silenceChanges(doc: SessionDoc, ids: ReadonlySet<string>, start: number, end: number): ClipChange[] {
  const media = new Map(doc.media.map((m) => [m.id, m]));
  const ramp = Math.round(doc.sample_rate * SILENCE_RAMP_SECONDS);
  const out: ClipChange[] = [];
  for (const t of doc.tracks) {
    for (const c of (t.clips ?? []) as ClipDoc[]) {
      if (!ids.has(c.id) || c.locked) continue;
      const len = clipLength(c, media.get(c.media_id), doc.sample_rate);
      const next = silenceSpan(pointsOf(c, "volume"), len, start - c.start, end - c.start, ramp);
      if (next) out.push({ id: c.id, volume: next });
    }
  }
  return out;
}
