// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Waveform editor edits. The file is a one-track session whose clips
 * are the edit list, so every edit here is a session op: pure functions
 * from the document and a time range to the op that does the edit.
 * Ranges are in samples at the file's rate.
 */

import { clipLength, sessionEnd, type ClipChange, type ClipDoc, type EnvelopePointDoc, type MarkerChange, type MediaRefDoc, type Op, type SessionDoc } from "../../stores/session";
import { clipEnd, isolate, mediaOf, pieceOf, rippleDeleteRange } from "./clip-edit";
import { DB_MIN, levelAt, pointsOf } from "./envelope";

const trackIds = (doc: SessionDoc) => doc.tracks.map((t) => t.id);

/** Clamps a range to the file and orders it. Null when nothing is left. */
export function clampRange(doc: SessionDoc, a: number, b: number): [number, number] | null {
  const end = sessionEnd(doc);
  const lo = Math.max(0, Math.min(a, b));
  const hi = Math.min(end, Math.max(a, b));
  return hi > lo ? [lo, hi] : null;
}

/** Adds marker moves to an edit, so markers stay on the same audio. */
function withMarkers(op: Op | null, extra: Op[]): Op | null {
  if (!op || extra.length === 0) return op;
  return op.op === "batch" ? { op: "batch", ops: [...op.ops, ...extra] } : { op: "batch", ops: [op, ...extra] };
}

/**
 * Marker moves for removing `a..b`: later markers move left, markers
 * inside the cut land at its start (never silently deleted), and range
 * markers shrink by the part that went.
 */
export function markersAfterCut(doc: SessionDoc, a: number, b: number): Op[] {
  const len = b - a;
  const at = (p: number) => (p >= b ? p - len : p >= a ? a : p);
  const changes = doc.markers.flatMap((m): MarkerChange[] => {
    const start = at(m.position);
    const end = m.duration > 0 ? at(m.position + m.duration) : start;
    const duration = Math.max(0, end - start);
    return start !== m.position || duration !== m.duration ? [{ id: m.id, position: start, duration }] : [];
  });
  return changes.length ? [{ op: "set_markers", changes }] : [];
}

/** Marker moves for opening `len` samples at `at`: later markers move right, ranges across it grow. */
export function markersAfterInsert(doc: SessionDoc, at: number, len: number): Op[] {
  const changes = doc.markers.flatMap((m): MarkerChange[] => {
    if (m.position >= at) return [{ id: m.id, position: m.position + len }];
    if (m.duration > 0 && m.position + m.duration > at) return [{ id: m.id, duration: m.duration + len }];
    return [];
  });
  return changes.length ? [{ op: "set_markers", changes }] : [];
}

/** Marker changes for Crop to `a..b`: markers outside go (their audio is gone), the rest move to the new start. */
export function markersAfterCrop(doc: SessionDoc, a: number, b: number): Op[] {
  const gone = doc.markers.filter((m) => m.position + m.duration < a || m.position > b || (m.duration === 0 && m.position === b)).map((m) => m.id);
  const changes = doc.markers
    .filter((m) => !gone.includes(m.id))
    .map((m) => {
      const start = Math.max(m.position, a);
      const end = Math.min(m.position + m.duration, b);
      return { id: m.id, position: start - a, duration: Math.max(0, end - start) };
    })
    .filter((c) => {
      const m = doc.markers.find((x) => x.id === c.id)!;
      return c.position !== m.position || c.duration !== m.duration;
    });
  const out: Op[] = [];
  if (gone.length) out.push({ op: "remove_markers", ids: gone });
  if (changes.length) out.push({ op: "set_markers", changes });
  return out;
}

/** Delete: removes the range and closes the gap. */
export function deleteRange(doc: SessionDoc, a: number, b: number, newId: () => string): Op | null {
  return withMarkers(rippleDeleteRange(doc, trackIds(doc), a, b, newId), markersAfterCut(doc, a, b));
}

/** Every clip after `ops` (an isolate result) runs, by id. */
function afterSplit(doc: SessionDoc, ops: Op[]): Map<string, ClipDoc> {
  const clips = new Map<string, ClipDoc>();
  for (const t of doc.tracks) for (const c of t.clips) clips.set(c.id, c);
  for (const op of ops) {
    if (op.op === "remove_clips") for (const id of op.ids) clips.delete(id);
    if (op.op === "restore_clips") for (const [, c] of op.clips) clips.set(c.id, c);
  }
  return clips;
}

/** Crop: keeps only the range, which then starts at 0. */
export function cropTo(doc: SessionDoc, a: number, b: number, newId: () => string): Op | null {
  if (a <= 0 && b >= sessionEnd(doc)) return null;
  const { ops, inside } = isolate(doc, trackIds(doc), a, b, newId);
  const keep = new Set(inside);
  const clips = afterSplit(doc, ops);
  const outside = [...clips.keys()].filter((id) => !keep.has(id));
  const out: Op[] = [...ops];
  if (outside.length) out.push({ op: "remove_clips", ids: outside });
  if (a > 0) out.push({ op: "set_clips", changes: inside.map((id) => ({ id, start: clips.get(id)!.start - a })) });
  out.push(...markersAfterCrop(doc, a, b));
  return { op: "batch", ops: out };
}

/** Splits around the range, then applies `change` to every piece inside it. */
function onPieces(doc: SessionDoc, a: number, b: number, newId: () => string, change: (c: ClipDoc) => ClipChange | null): Op | null {
  const { ops, inside } = isolate(doc, trackIds(doc), a, b, newId);
  const pieces = afterSplit(doc, ops);
  const changes = inside.map((id) => change(pieces.get(id)!)).filter((c): c is ClipChange => c !== null);
  if (changes.length === 0) return null;
  return { op: "batch", ops: [...ops, { op: "set_clips", changes }] };
}

/** Silence: the range plays nothing; its length stays. */
export function silenceRange(doc: SessionDoc, a: number, b: number, newId: () => string): Op | null {
  return onPieces(doc, a, b, newId, (c) => (c.muted ? null : { id: c.id, muted: true }));
}

/** Amplify: raises or lowers the range by `db`. */
export function amplifyRange(doc: SessionDoc, a: number, b: number, db: number, newId: () => string): Op | null {
  if (db === 0) return null;
  return onPieces(doc, a, b, newId, (c) => ({ id: c.id, gain_db: Math.max(-96, Math.min(24, c.gain_db + db)) }));
}

/** Gain in dB for a fade at `t` from 0 to 1: an equal-power curve, silent at 0. */
export function fadeDb(t: number): number {
  const g = Math.sin((Math.max(0, Math.min(1, t)) * Math.PI) / 2);
  return g <= 0 ? DB_MIN : Math.max(DB_MIN, 20 * Math.log10(g));
}

const FADE_POINTS = 24;

/**
 * Fade In or Fade Out over the range: written into each piece's volume
 * envelope (on top of any envelope it already has), so a fade can cross
 * earlier edits.
 */
export function fadeRange(doc: SessionDoc, a: number, b: number, dir: "in" | "out", newId: () => string): Op | null {
  const media = mediaOf(doc);
  const span = b - a;
  if (span <= 0) return null;
  return onPieces(doc, a, b, newId, (c) => {
    const m = media.get(c.media_id);
    const len = clipLength(c, m, doc.sample_rate);
    const old = pointsOf(c, "volume");
    const pts: EnvelopePointDoc[] = [];
    for (let i = 0; i <= FADE_POINTS; i++) {
      const pos = Math.round((len * i) / FADE_POINTS);
      const t = (c.start + pos - a) / span;
      const f = fadeDb(dir === "in" ? t : 1 - t);
      pts.push({ position: pos, value: Math.max(DB_MIN, levelAt(old, pos) + f), curve: "linear" });
    }
    return { id: c.id, volume: pts };
  });
}

/** Copied audio: pieces placed from 0, plus the files they play. */
export interface WaveBoard {
  rate: number;
  length: number;
  clips: ClipDoc[];
  media: MediaRefDoc[];
  /** Copied with only this channel on (0 left, 1 right): pasting uses that channel. */
  channel?: number;
}

/** Copy: the range as pieces starting at 0. */
export function copyRange(doc: SessionDoc, a: number, b: number): WaveBoard | null {
  const media = mediaOf(doc);
  const rate = doc.sample_rate;
  const clips: ClipDoc[] = [];
  for (const t of doc.tracks) {
    for (const c of t.clips) {
      const end = clipEnd(c, media, rate);
      if (end <= a || c.start >= b) continue;
      const from = Math.max(a, c.start) - c.start;
      const to = Math.min(b, end) - c.start;
      clips.push(pieceOf(c, media.get(c.media_id), rate, from, to, c.start + from - a, c.id));
    }
  }
  if (clips.length === 0) return null;
  const used = new Set(clips.map((c) => c.media_id));
  return { rate, length: b - a, clips, media: doc.media.filter((m) => used.has(m.id)) };
}

/**
 * Mix Paste: lays the copied audio over what is already there from
 * `at`, `db` louder or quieter, without moving anything. The file grows
 * if the pasted audio runs past its end.
 */
export function mixPasteAt(doc: SessionDoc, board: WaveBoard, at: number, db: number, newId: () => string): { op: Op; length: number } | null {
  const track = doc.tracks[0];
  if (!track || board.clips.length === 0) return null;
  const ratio = doc.sample_rate / board.rate;
  const media = mediaOf(doc);
  const ops: Op[] = [];
  for (const m of board.media) if (!media.has(m.id)) ops.push({ op: "add_media", media: m });
  const clips: [string, ClipDoc][] = board.clips.map((c) => [
    track.id,
    { ...c, id: newId(), start: at + Math.round(c.start * ratio), gain_db: Math.max(-96, Math.min(24, c.gain_db + db)) },
  ]);
  ops.push({ op: "restore_clips", clips });
  return { op: { op: "batch", ops }, length: Math.round(board.length * ratio) };
}

/**
 * Paste: opens a gap at `at` as long as the copied audio and puts it
 * there. Audio copied from a file at another rate keeps its timing.
 */
export function pasteAt(doc: SessionDoc, board: WaveBoard, at: number, newId: () => string): { op: Op; length: number } | null {
  const track = doc.tracks[0];
  if (!track || board.clips.length === 0) return null;
  const ratio = doc.sample_rate / board.rate;
  const length = Math.round(board.length * ratio);
  const media = mediaOf(doc);
  const ops: Op[] = [];
  for (const m of board.media) if (!media.has(m.id)) ops.push({ op: "add_media", media: m });
  // Split whatever spans `at`, then move everything from `at` on to the right.
  const { ops: split } = isolate(doc, trackIds(doc), at, Number.MAX_SAFE_INTEGER, newId);
  const moved = new Map<string, ClipDoc>();
  for (const t of doc.tracks) for (const c of t.clips) moved.set(c.id, c);
  const restored: [string, ClipDoc][] = [];
  for (const op of split) {
    if (op.op === "remove_clips") ops.push(op);
    if (op.op === "restore_clips") {
      for (const [tid, c] of op.clips) {
        moved.delete(c.id);
        restored.push([tid, c.start >= at ? { ...c, start: c.start + length } : c]);
      }
    }
  }
  const shift: ClipChange[] = [];
  for (const c of moved.values()) if (c.start >= at) shift.push({ id: c.id, start: c.start + length });
  if (shift.length) ops.push({ op: "set_clips", changes: shift });
  for (const c of board.clips) restored.push([track.id, { ...c, id: newId(), start: at + Math.round(c.start * ratio) }]);
  ops.push({ op: "restore_clips", clips: restored });
  ops.push(...markersAfterInsert(doc, at, length));
  return { op: { op: "batch", ops }, length };
}

/**
 * Replace: swaps the audio in `a..b` for `media`, a file of exactly that
 * length (a spectral edit's result). The rest of the file stays put.
 */
export function replaceRange(doc: SessionDoc, a: number, b: number, media: MediaRefDoc, name: string, newId: () => string): Op | null {
  const track = doc.tracks[0];
  if (!track || b <= a) return null;
  const { ops, inside } = isolate(doc, trackIds(doc), a, b, newId);
  const out: Op[] = [...ops];
  if (inside.length) out.push({ op: "remove_clips", ids: inside });
  out.push({ op: "add_media", media });
  out.push({ op: "add_clips", track_id: track.id, clips: [{ id: newId(), name, media_id: media.id, start: a, source_in: 0, source_out: media.length }] });
  return { op: "batch", ops: out };
}

/**
 * Stretch: puts `media` (the range re-timed) in place of `a..b` and moves
 * everything after `b` by the change in length, so nothing overlaps or
 * leaves a gap. Markers after the range move with it; markers inside are
 * spaced out (or in) to match.
 */
export function stretchRange(doc: SessionDoc, a: number, b: number, media: MediaRefDoc, name: string, newId: () => string): Op | null {
  const track = doc.tracks[0];
  if (!track || b <= a || media.length <= 0) return null;
  const length = Math.round((media.length * doc.sample_rate) / media.sample_rate);
  const delta = length - (b - a);
  const { ops, inside } = isolate(doc, trackIds(doc), a, b, newId);
  const out: Op[] = [...ops];
  if (inside.length) out.push({ op: "remove_clips", ids: inside });
  const gone = new Set(inside);
  const clips = afterSplit(doc, ops);
  const shift: ClipChange[] = [];
  for (const c of clips.values()) if (!gone.has(c.id) && c.start >= b && delta !== 0) shift.push({ id: c.id, start: c.start + delta });
  // Moving right, the furthest clip goes first; moving left, the nearest.
  shift.sort((x, y) => (delta > 0 ? y.start! - x.start! : x.start! - y.start!));
  if (shift.length) out.push({ op: "set_clips", changes: shift });
  out.push({ op: "add_media", media });
  out.push({ op: "add_clips", track_id: track.id, clips: [{ id: newId(), name, media_id: media.id, start: a, source_in: 0, source_out: media.length }] });
  const ratio = length / (b - a);
  const markers = doc.markers.flatMap((m): MarkerChange[] => {
    if (m.position >= b) return delta ? [{ id: m.id, position: m.position + delta }] : [];
    if (m.position >= a) return [{ id: m.id, position: a + Math.round((m.position - a) * ratio), duration: Math.round(m.duration * ratio) }];
    return [];
  });
  if (markers.length) out.push({ op: "set_markers", changes: markers });
  return { op: "batch", ops: out };
}

/** Insert: puts a whole file (a recorded take) in at `at`, moving what follows to the right. */
export function insertFile(doc: SessionDoc, at: number, media: MediaRefDoc, name: string, newId: () => string): { op: Op; length: number } | null {
  const track = doc.tracks[0];
  if (!track || media.length <= 0) return null;
  const length = Math.round((media.length * doc.sample_rate) / media.sample_rate);
  const ops: Op[] = [{ op: "add_media", media }];
  const { ops: split } = isolate(doc, trackIds(doc), at, Number.MAX_SAFE_INTEGER, newId);
  const moved = new Map<string, ClipDoc>();
  for (const t of doc.tracks) for (const c of t.clips) moved.set(c.id, c);
  const restored: [string, ClipDoc][] = [];
  for (const op of split) {
    if (op.op === "remove_clips") ops.push(op);
    if (op.op === "restore_clips") {
      for (const [tid, c] of op.clips) {
        moved.delete(c.id);
        restored.push([tid, c.start >= at ? { ...c, start: c.start + length } : c]);
      }
    }
  }
  const shift: ClipChange[] = [];
  for (const c of moved.values()) if (c.start >= at) shift.push({ id: c.id, start: c.start + length });
  if (shift.length) ops.push({ op: "set_clips", changes: shift });
  if (restored.length) ops.push({ op: "restore_clips", clips: restored });
  ops.push({ op: "add_clips", track_id: track.id, clips: [{ id: newId(), name, media_id: media.id, start: at, source_in: 0, source_out: media.length }] });
  ops.push(...markersAfterInsert(doc, at, length));
  return { op: { op: "batch", ops }, length };
}
