// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Recording options and where a finished take lands.
 *
 * Offset: audio interfaces deliver what the mic heard a little late, so
 * a take sits slightly behind the music it was played against. The
 * offset trims that much from the start of each take so it lines up.
 *
 * Punch-in: with a time selection, recording starts a little before it
 * (pre-roll, so the talent hears the lead-in), stops by itself at the
 * end, keeps only the selected range, and replaces what was there.
 */

export interface RecordOptions {
  /** Milliseconds trimmed from the start of every take. */
  offsetMs: number;
  /** Record into the time selection when there is one. */
  punch: boolean;
  /** Seconds of playback before a punch-in. */
  prerollSec: number;
  /** A punch-in replaces the audio already in that range on the recorded track. */
  replace: boolean;
}

export const RECORD_DEFAULTS: RecordOptions = { offsetMs: 0, punch: true, prerollSec: 2, replace: true };

const num = (v: unknown, lo: number, hi: number, d: number) => (typeof v === "number" && Number.isFinite(v) ? Math.min(hi, Math.max(lo, v)) : d);

export function sanitizeRecordOptions(raw: unknown): RecordOptions {
  const r = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  return {
    offsetMs: num(r.offsetMs, 0, 1000, RECORD_DEFAULTS.offsetMs),
    punch: r.punch !== false,
    prerollSec: num(r.prerollSec, 0, 10, RECORD_DEFAULTS.prerollSec),
    replace: r.replace !== false,
  };
}

export interface RecordPlan {
  /** Session sample where recording (and playback) starts. */
  at: number;
  /** The range a punch-in keeps, or null for an ordinary take. */
  punch: { start: number; end: number } | null;
}

/** Where to start, given the playhead and the time selection. */
export function recordPlan(playhead: number, selection: { start: number; end: number } | null, o: RecordOptions, rate: number): RecordPlan {
  if (o.punch && selection && selection.end > selection.start) {
    return { at: Math.max(0, selection.start - Math.round(o.prerollSec * rate)), punch: { ...selection } };
  }
  return { at: playhead, punch: null };
}

/** Seconds of recording after which a punch-in stops by itself. */
export function punchStopAfter(p: RecordPlan, rate: number): number | null {
  return p.punch ? (p.punch.end - p.at) / rate : null;
}

/**
 * Where a take of `length` source frames at `takeRate` lands: its clip
 * start and source range. `startFrames` is how far into the recording
 * the take began (a track armed while recording). Null when nothing
 * usable is left.
 */
export function placeTake(
  p: RecordPlan,
  take: { length: number; sampleRate: number; startFrames?: number },
  sessionRate: number,
  offsetMs: number,
): { start: number; sourceIn: number; sourceOut: number } | null {
  const toSrc = (s: number) => Math.round((s * take.sampleRate) / sessionRate);
  const offset = Math.round((offsetMs / 1000) * take.sampleRate);
  // Where this take's first frame sits in the session.
  const at = p.at + Math.round(((take.startFrames ?? 0) * sessionRate) / take.sampleRate);
  let start = at;
  let sourceIn = offset;
  let sourceOut = take.length;
  if (p.punch) {
    if (at >= p.punch.end) return null;
    start = Math.max(p.punch.start, at);
    sourceIn = offset + toSrc(start - at);
    sourceOut = Math.min(take.length, offset + toSrc(p.punch.end - at));
  }
  return sourceOut > sourceIn ? { start, sourceIn, sourceOut } : null;
}
