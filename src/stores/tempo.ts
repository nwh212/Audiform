// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

/**
 * The session's tempo map: the tempo and time signature it starts with,
 * and changes at later bars. Beats here are quarter notes, as in MIDI
 * clips; a bar of 6/8 is 3 of them. The same sums as the core's
 * `Tempo` (session/model.rs), so what is drawn is what plays.
 */

export interface TempoChangeDoc {
  id: string;
  /** Bars from the session start where it takes over (1 is the second bar). */
  bar: number;
  bpm: number;
  beats_per_bar: number;
  beat_unit: number;
}

export interface TempoDoc {
  /** Quarter notes per minute. */
  bpm: number;
  /** The top of the time signature. */
  beats_per_bar: number;
  /** The bottom of the time signature (4 when left out). */
  beat_unit?: number;
  changes?: TempoChangeDoc[];
}

export const DEFAULT_TEMPO: TempoDoc = { bpm: 120, beats_per_bar: 4 };
export const BPM_MIN = 20;
export const BPM_MAX = 400;
/** Bottoms a time signature can have. */
export const BEAT_UNITS = [1, 2, 4, 8, 16, 32] as const;

/** One stretch of the map with one tempo and time signature. */
export interface TempoSegment {
  /** Bars from the session start where it begins. */
  bar: number;
  /** Quarter notes from the start where it begins. */
  beat: number;
  /** Seconds from the start where it begins. */
  seconds: number;
  bpm: number;
  beats_per_bar: number;
  beat_unit: number;
  /** The change it comes from; null for the start. */
  id: string | null;
}

const clampBpm = (b: number) => (Number.isFinite(b) ? Math.min(BPM_MAX, Math.max(BPM_MIN, b)) : 120);

/** Quarter notes in one bar of a segment. */
export const barBeats = (s: { beats_per_bar: number; beat_unit?: number }) => (Math.max(1, s.beats_per_bar) * 4) / Math.max(1, s.beat_unit ?? 4);
/** Quarter notes in one beat of its time signature (0.5 in 6/8). */
export const unitBeats = (s: { beat_unit?: number }) => 4 / Math.max(1, s.beat_unit ?? 4);

const cache = new WeakMap<TempoDoc, TempoSegment[]>();

/** The stretches of the map in order. Never empty. */
export function segments(t: TempoDoc): TempoSegment[] {
  const hit = cache.get(t);
  if (hit) return hit;
  const out: TempoSegment[] = [
    { bar: 0, beat: 0, seconds: 0, bpm: clampBpm(t.bpm), beats_per_bar: Math.max(1, t.beats_per_bar), beat_unit: Math.max(1, t.beat_unit ?? 4), id: null },
  ];
  for (const c of t.changes ?? []) {
    const p = out[out.length - 1]!;
    if (c.bar <= p.bar) continue;
    const beat = p.beat + (c.bar - p.bar) * barBeats(p);
    out.push({
      bar: c.bar,
      beat,
      seconds: p.seconds + ((beat - p.beat) * 60) / p.bpm,
      bpm: clampBpm(c.bpm),
      beats_per_bar: Math.max(1, c.beats_per_bar),
      beat_unit: Math.max(1, c.beat_unit),
      id: c.id,
    });
  }
  cache.set(t, out);
  return out;
}

function lastWhere(segs: TempoSegment[], ok: (s: TempoSegment) => boolean): TempoSegment {
  let seg = segs[0]!;
  for (let i = 1; i < segs.length && ok(segs[i]!); i++) seg = segs[i]!;
  return seg;
}

export const segmentAtBeat = (t: TempoDoc, beats: number) => lastWhere(segments(t), (s) => s.beat <= beats);
export const segmentAtSeconds = (t: TempoDoc, secs: number) => lastWhere(segments(t), (s) => s.seconds <= secs);
export const segmentAtBar = (t: TempoDoc, bar: number) => lastWhere(segments(t), (s) => s.bar <= bar);

/** Quarter notes to seconds. */
export function beatsToSeconds(t: TempoDoc, beats: number): number {
  const b = Math.max(0, beats);
  const s = segmentAtBeat(t, b);
  return s.seconds + ((b - s.beat) * 60) / s.bpm;
}

/** Seconds to quarter notes. */
export function secondsToBeats(t: TempoDoc, secs: number): number {
  const s = segmentAtSeconds(t, secs);
  return s.beat + ((secs - s.seconds) * s.bpm) / 60;
}

/** Where a bar starts (bars from 0), in quarter notes. */
export function barStart(t: TempoDoc, bar: number): number {
  const s = segmentAtBar(t, bar);
  return s.beat + (bar - s.bar) * barBeats(s);
}

/** The bar (from 0) holding a time in quarter notes, and how far into it, in beats of its time signature. */
export function barAt(t: TempoDoc, beats: number): { bar: number; beat: number; seg: TempoSegment } {
  const seg = segmentAtBeat(t, beats);
  const into = Math.max(0, beats - seg.beat);
  const per = barBeats(seg);
  // A hair of slack so a bar line computed a little short counts as the new bar.
  const bars = Math.floor(into / per + 1e-9);
  return { bar: seg.bar + bars, beat: Math.max(0, (into - bars * per) / unitBeats(seg)), seg };
}

/** The nearest bar line to a time in quarter notes, as a bar number from 0. */
export function nearestBar(t: TempoDoc, beats: number): number {
  const { bar } = barAt(t, beats);
  const a = barStart(t, bar);
  const b = barStart(t, bar + 1);
  return beats - a <= b - beats ? bar : bar + 1;
}

/** "120" or "92.5". */
export const bpmText = (bpm: number) => String(Math.round(bpm * 100) / 100);
/** "4/4", "6/8". */
export const meterText = (s: { beats_per_bar: number; beat_unit?: number }) => `${s.beats_per_bar}/${s.beat_unit ?? 4}`;

/** Reads "6/8" or "3" (over 4). Null when it is not a time signature. */
export function parseMeter(text: string): { beats_per_bar: number; beat_unit: number } | null {
  const m = /^\s*(\d{1,2})\s*(?:\/\s*(\d{1,2}))?\s*$/.exec(text);
  if (!m) return null;
  const top = Number(m[1]);
  const bottom = m[2] === undefined ? 4 : Number(m[2]);
  if (top < 1 || top > 32 || !(BEAT_UNITS as readonly number[]).includes(bottom)) return null;
  return { beats_per_bar: top, beat_unit: bottom };
}

/** The map with the stretch `seg` (the start, or a change) given new values. */
export function editSegment(t: TempoDoc, seg: TempoSegment, values: { bpm?: number; beats_per_bar?: number; beat_unit?: number }): TempoDoc {
  if (seg.id === null) return { ...t, ...values, changes: t.changes ?? [] };
  return { ...t, changes: (t.changes ?? []).map((c) => (c.id === seg.id ? { ...c, ...values } : c)) };
}

/** The map with a change at `bar` (from 1), taking the values in effect there; the existing one if there is one already. */
export function addChange(t: TempoDoc, bar: number, id: string): { tempo: TempoDoc; id: string } {
  const at = Math.max(1, Math.round(bar));
  const existing = (t.changes ?? []).find((c) => c.bar === at);
  if (existing) return { tempo: t, id: existing.id };
  const s = segmentAtBar(t, at);
  const c: TempoChangeDoc = { id, bar: at, bpm: s.bpm, beats_per_bar: s.beats_per_bar, beat_unit: s.beat_unit };
  return { tempo: { ...t, changes: [...(t.changes ?? []), c].sort((a, b) => a.bar - b.bar) }, id };
}

/** The map with a change moved to another bar (from 1). Moving onto another change replaces it. */
export function moveChange(t: TempoDoc, id: string, bar: number): TempoDoc {
  const at = Math.max(1, Math.round(bar));
  const changes = (t.changes ?? []).filter((c) => c.id === id || c.bar !== at).map((c) => (c.id === id ? { ...c, bar: at } : c));
  return { ...t, changes: changes.sort((a, b) => a.bar - b.bar) };
}

export function removeChange(t: TempoDoc, id: string): TempoDoc {
  return { ...t, changes: (t.changes ?? []).filter((c) => c.id !== id) };
}
