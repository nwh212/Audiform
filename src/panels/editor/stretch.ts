// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Time stretch edits: fit a clip to a length (an ad slot, a time
 * selection) or a percentage, and remove a stretch. The clip keeps its
 * source range; only how long it plays changes. Envelope points and
 * fades follow the new length so they stay on the same words.
 */

import { clipLength, stretchOf, STRETCH_MAX, STRETCH_MIN, type ClipChange, type ClipDoc, type EnvelopePointDoc, type MediaRefDoc } from "../../stores/session";

/** Beyond this much either way, stretched speech starts to sound processed. */
export const NATURAL_LIMIT = 0.25;

/** Length the clip would play at with no stretch, in session samples. */
export function naturalLength(clip: ClipDoc, m: MediaRefDoc | undefined, rate: number): number {
  return clipLength({ ...clip, stretch: null }, m, rate);
}

function scaleEnvelopes(clip: ClipDoc, by: number): Pick<ClipChange, "volume" | "pan"> {
  const out: Pick<ClipChange, "volume" | "pan"> = {};
  for (const e of clip.envelopes ?? []) {
    if (!e.enabled) continue;
    const pts: EnvelopePointDoc[] = e.points.map((p) => ({ ...p, position: Math.round(p.position * by) }));
    if (e.target.type === "volume") out.volume = pts;
    if (e.target.type === "pan") out.pan = pts;
  }
  return out;
}

export interface StretchResult {
  change: ClipChange;
  ratio: number;
  /** True when the ratio had to be limited to what a clip can take. */
  limited: boolean;
}

/**
 * The change that makes `clip` play for `length` session samples.
 * Null when the clip loops (loops cannot stretch) or nothing changes.
 */
export function stretchTo(clip: ClipDoc, m: MediaRefDoc | undefined, rate: number, length: number, keepPitch: boolean): StretchResult | null {
  if (clip.looped || length <= 0) return null;
  const natural = naturalLength(clip, m, rate);
  if (natural <= 0) return null;
  const want = length / natural;
  const ratio = Math.max(STRETCH_MIN, Math.min(STRETCH_MAX, want));
  const before = stretchOf(clip);
  const newLen = Math.round(natural * ratio);
  if (Math.abs(ratio - before) < 1e-6 && (clip.stretch?.keep_pitch ?? true) === keepPitch) return null;
  const by = ratio / before;
  // Pitch, type and finer settings stay; a pitch change keeps the stretch even at 100%.
  const kept = { ...clip.stretch, ratio, keep_pitch: keepPitch };
  const change: ClipChange =
    Math.abs(ratio - 1) < 1e-6 && !(keepPitch && kept.pitch)
      ? { id: clip.id, unstretch: true }
      : { id: clip.id, stretch: kept };
  Object.assign(change, scaleEnvelopes(clip, by));
  const fi = Math.min(Math.round(clip.fade_in.length * by), newLen);
  const fo = Math.min(Math.round(clip.fade_out.length * by), newLen - fi);
  if (fi !== clip.fade_in.length) change.fade_in = { ...clip.fade_in, length: fi };
  if (fo !== clip.fade_out.length) change.fade_out = { ...clip.fade_out, length: fo };
  return { change, ratio, limited: Math.abs(ratio - want) > 1e-6 };
}

/** Back to the natural length and pitch. Null when the clip is not stretched. */
export function unstretch(clip: ClipDoc, m: MediaRefDoc | undefined, rate: number): ClipChange | null {
  if (!clip.stretch) return null;
  // Back to natural speed and pitch: the fades and envelopes scale back too.
  const change = stretchTo(clip, m, rate, naturalLength(clip, m, rate), clip.stretch.keep_pitch)?.change ?? { id: clip.id };
  delete change.stretch;
  change.unstretch = true;
  return change;
}

/** "112.5%" style label for a ratio. */
export function percent(ratio: number): string {
  return `${Math.round(ratio * 1000) / 10}%`;
}

/** Parses "30", "30.5", "1:05", "1:05.250" or "0:01:05" into seconds. Null when it is not a time. */
export function parseSeconds(text: string): number | null {
  const t = text.trim();
  if (!/^\d+(:\d{1,2}){0,2}(\.\d+)?$/.test(t)) return null;
  const parts = t.split(":").map(Number);
  let secs = 0;
  for (const p of parts) secs = secs * 60 + p;
  return Number.isFinite(secs) && secs > 0 ? secs : null;
}
