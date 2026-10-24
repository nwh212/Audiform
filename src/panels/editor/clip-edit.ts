// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

/**
 * Clip editing math: moving, trimming, fading and splitting. Pure
 * functions that turn a gesture into session ops, so the rules are
 * tested without a window.
 *
 * Timeline values are session samples. Source values are samples at the
 * file's own rate.
 */

import { settings } from "../../stores/settings";
import { clipLength, stretchOf, type ClipChange, type ClipDoc, type EnvelopePointDoc, type MediaRefDoc, type Op, type SessionDoc, type TrackDoc } from "../../stores/session";
import { pointsOf, sliceVolume, type EnvKind } from "./envelope";

/**
 * The id a clip's envelope uses, as in the core: the clip id for
 * volume, and the clip id with its lowest bit flipped for pan.
 */
export function envelopeId(clipId: string, kind: "volume" | "pan"): string {
  if (kind === "volume") return clipId;
  const last = parseInt(clipId.slice(-1), 16) ^ 1;
  return clipId.slice(0, -1) + last.toString(16);
}

/** A copy of a clip with one envelope's points replaced. Empty removes it. */
export function withEnvelope(clip: ClipDoc, kind: "volume" | "pan", points: EnvelopePointDoc[]): ClipDoc {
  const others = (clip.envelopes ?? []).filter((e) => e.target.type !== kind);
  return {
    ...clip,
    envelopes: points.length ? [...others, { id: envelopeId(clip.id, kind), target: { type: kind }, enabled: true, points }] : others,
  };
}

export function withVolume(clip: ClipDoc, points: EnvelopePointDoc[]): ClipDoc {
  return withEnvelope(clip, "volume", points);
}

type Slices = { volume?: EnvelopePointDoc[]; pan?: EnvelopePointDoc[]; effect_points?: { slot_id: string; param: string; points: EnvelopePointDoc[] }[] };

/** Envelope points for the piece of `clip` from `from` to `to`; only envelopes it has. */
function sliceOf(clip: ClipDoc, from: number, to: number): Slices {
  const out: Slices = {};
  for (const kind of ["volume", "pan"] as const) {
    const pts = pointsOf(clip, kind);
    if (pts.length) out[kind] = sliceVolume(pts, from, to);
  }
  // Effect settings over time move with the audio too.
  const fx = (clip.envelopes ?? []).flatMap((e) =>
    e.target.type === "effect_param" && e.enabled && e.points.length
      ? [{ slot_id: e.target.slot_id, param: e.target.param, points: sliceVolume(e.points, from, to) }]
      : [],
  );
  if (fx.length) out.effect_points = fx;
  return out;
}

/** A new clip with the given envelope slices applied. */
function withSlices(clip: ClipDoc, slices: Slices): ClipDoc {
  let c = clip;
  if (slices.volume) c = withEnvelope(c, "volume", slices.volume);
  if (slices.pan) c = withEnvelope(c, "pan", slices.pan);
  for (const f of slices.effect_points ?? []) {
    c = {
      ...c,
      envelopes: (c.envelopes ?? []).map((e) =>
        e.target.type === "effect_param" && e.target.slot_id === f.slot_id && e.target.param === f.param ? { ...e, points: f.points } : e,
      ),
    };
  }
  return c;
}

const mod = (x: number, r: number) => ((x % r) + r) % r;

/**
 * What changes when a clip loses (or, with a negative `d`, gains) `d`
 * session samples on its left: a later source start, or for a looped
 * clip a shorter loop starting further into the region.
 */
function cutLeft(clip: ClipDoc, m: MediaRefDoc | undefined, rate: number, d: number): Pick<ClipChange, "source_in" | "looped"> {
  const len = clipLength(clip, m, rate);
  if (clip.looped) {
    const region = Math.max(1, clip.source_out - clip.source_in);
    return { looped: { length: len - d, phase: mod(clip.looped.phase + toSource(d, m, rate), region) } };
  }
  return { source_in: Math.max(0, clip.source_in + toSource(d, m, rate, stretchOf(clip))) };
}

/** What changes when a clip becomes `newLen` session samples long, keeping its left edge. */
function cutRight(clip: ClipDoc, m: MediaRefDoc | undefined, rate: number, newLen: number): Pick<ClipChange, "source_out" | "looped"> {
  if (clip.looped) return { looped: { ...clip.looped, length: newLen } };
  let out = clip.source_in + toSource(newLen, m, rate, stretchOf(clip));
  if (m && m.length > 0) out = Math.min(out, m.length);
  return { source_out: out };
}

/** Shortest clip a trim can leave, in session samples (about 10 ms at 48 kHz). */
export const MIN_CLIP = 480;

export interface Located {
  clip: ClipDoc;
  track: TrackDoc;
  trackIndex: number;
}

export function mediaOf(doc: SessionDoc): Map<string, MediaRefDoc> {
  return new Map(doc.media.map((m) => [m.id, m]));
}

export function findClips(doc: SessionDoc, ids: Iterable<string>): Located[] {
  const want = new Set(ids);
  const out: Located[] = [];
  doc.tracks.forEach((track, trackIndex) => {
    for (const clip of track.clips) if (want.has(clip.id)) out.push({ clip, track, trackIndex });
  });
  return out;
}

export function clipEnd(clip: ClipDoc, media: ReadonlyMap<string, MediaRefDoc>, rate: number): number {
  return clip.start + clipLength(clip, media.get(clip.media_id), rate);
}

/** Session samples to source samples for a clip's file. A stretched clip covers less source per sample. */
function toSource(samples: number, m: MediaRefDoc | undefined, rate: number, ratio = 1): number {
  const sr = m?.sample_rate || rate;
  return Math.round((samples * sr) / rate / ratio);
}

/** Edges worth snapping to: session start, playhead, and every other clip's ends. */
export function snapTargets(
  doc: SessionDoc,
  exclude: ReadonlySet<string>,
  playhead: number,
  use: { clips: boolean; playhead: boolean; markers?: boolean } = { clips: true, playhead: true, markers: true },
): number[] {
  const media = mediaOf(doc);
  const out = use.playhead ? [0, playhead] : [0];
  if (use.markers !== false) for (const m of doc.markers ?? []) out.push(m.position, m.position + m.duration);
  if (!use.clips) return out;
  for (const t of doc.tracks)
    for (const c of t.clips) {
      if (exclude.has(c.id)) continue;
      out.push(c.start, clipEnd(c, media, doc.sample_rate));
    }
  return out;
}

/** The target nearest `value` within `tolerance`, or null. */
export function nearest(value: number, targets: readonly number[], tolerance: number): number | null {
  let best: number | null = null;
  for (const t of targets) {
    const d = Math.abs(t - value);
    if (d <= tolerance && (best === null || d < Math.abs(best - value))) best = t;
  }
  return best;
}

export interface MovePreview {
  /** Applied timeline shift in samples, after clamping and snapping. */
  delta: number;
  /** Applied track shift in rows. */
  rows: number;
  changes: ClipChange[];
}

/**
 * Moves clips together by `delta` samples and `rows` tracks. The group
 * keeps its shape: it stops at the session start and at the first and
 * last track. Either edge of the group snaps to `targets` within
 * `tolerance`. Locked clips stay where they are.
 */
export function moveClips(
  doc: SessionDoc,
  ids: Iterable<string>,
  delta: number,
  rows: number,
  targets: readonly number[] = [],
  tolerance = 0,
): MovePreview {
  const media = mediaOf(doc);
  const items = findClips(doc, ids).filter((l) => !l.clip.locked);
  if (items.length === 0) return { delta: 0, rows: 0, changes: [] };
  const first = Math.min(...items.map((l) => l.clip.start));
  const last = Math.max(...items.map((l) => clipEnd(l.clip, media, doc.sample_rate)));
  let d = Math.round(delta);
  if (tolerance > 0) {
    const a = nearest(first + d, targets, tolerance);
    const b = nearest(last + d, targets, tolerance);
    const da = a === null ? Infinity : a - first - d;
    const db = b === null ? Infinity : b - last - d;
    if (Math.abs(da) <= Math.abs(db) && da !== Infinity) d += da;
    else if (db !== Infinity) d += db;
  }
  d = Math.max(d, -first);
  const top = Math.min(...items.map((l) => l.trackIndex));
  const bottom = Math.max(...items.map((l) => l.trackIndex));
  const r = Math.max(-top, Math.min(doc.tracks.length - 1 - bottom, Math.round(rows)));
  const changes: ClipChange[] = [];
  for (const l of items) {
    const ch: ClipChange = { id: l.clip.id };
    if (d !== 0) ch.start = l.clip.start + d;
    if (r !== 0) ch.track_id = doc.tracks[l.trackIndex + r]!.id;
    if (ch.start !== undefined || ch.track_id !== undefined) changes.push(ch);
  }
  return { delta: d, rows: r, changes };
}

/**
 * Drags one edge of a clip by `delta` samples. The left edge moves the
 * clip start and its source start together, so the audio stays put in
 * time. Neither edge can pass the ends of the file or make the clip
 * shorter than `MIN_CLIP`. The dragged edge snaps to `targets`.
 */
export function trimClip(
  doc: SessionDoc,
  clip: ClipDoc,
  edge: "left" | "right",
  delta: number,
  targets: readonly number[] = [],
  tolerance = 0,
): ClipChange | null {
  if (clip.locked) return null;
  const rate = doc.sample_rate;
  const m = doc.media.find((x) => x.id === clip.media_id);
  const sr = m?.sample_rate || rate;
  const len = clipLength(clip, m, rate);
  const end = clip.start + len;
  if (edge === "left") {
    let at = clip.start + Math.round(delta);
    const snapped = tolerance > 0 ? nearest(at, targets, tolerance) : null;
    if (snapped !== null) at = snapped;
    // How far left the source allows (a looped clip can grow to the session start).
    const earliest = clip.looped ? 0 : Math.max(0, clip.start - Math.floor(((clip.source_in * rate) / sr) * stretchOf(clip)));
    at = Math.min(Math.max(at, earliest), end - MIN_CLIP);
    const d = at - clip.start;
    if (d === 0) return null;
    const fadeIn = Math.max(0, Math.min(clip.fade_in.length, end - at));
    return {
      id: clip.id,
      start: at,
      ...cutLeft(clip, m, rate, d),
      ...(fadeIn !== clip.fade_in.length ? { fade_in: { ...clip.fade_in, length: fadeIn } } : {}),
      ...sliceOf(clip, d, len),
    };
  }
  let at = end + Math.round(delta);
  const snapped = tolerance > 0 ? nearest(at, targets, tolerance) : null;
  if (snapped !== null) at = snapped;
  const fileEnd = !clip.looped && m && m.length > 0 ? clip.start + Math.floor((((m.length - clip.source_in) * rate) / sr) * stretchOf(clip)) : Infinity;
  at = Math.max(clip.start + MIN_CLIP, Math.min(at, fileEnd));
  if (at === end) return null;
  const fadeOut = Math.max(0, Math.min(clip.fade_out.length, at - clip.start));
  return {
    id: clip.id,
    ...cutRight(clip, m, rate, at - clip.start),
    ...(fadeOut !== clip.fade_out.length ? { fade_out: { ...clip.fade_out, length: fadeOut } } : {}),
  };
}

/**
 * Sets a fade length in samples. A fade can cover the whole clip, but
 * the fade in and fade out together never overlap.
 */
export function fadeClip(doc: SessionDoc, clip: ClipDoc, which: "in" | "out", length: number): ClipChange | null {
  const len = clipLength(clip, doc.media.find((x) => x.id === clip.media_id), doc.sample_rate);
  const other = which === "in" ? clip.fade_out.length : clip.fade_in.length;
  const l = Math.max(0, Math.min(Math.round(length), len - other));
  const cur = which === "in" ? clip.fade_in : clip.fade_out;
  if (l === cur.length) return null;
  // A new fade takes the shape picked in Settings > General.
  const shape = cur.length === 0 && cur.shape === "linear" ? settings.current.fadeShape : cur.shape;
  const fade = { ...cur, length: l, shape };
  return which === "in" ? { id: clip.id, fade_in: fade } : { id: clip.id, fade_out: fade };
}

/**
 * Splits clips at `at`. Each clip that spans `at` keeps its left part
 * and gains a new clip for the right part. The fade in stays on the
 * left, the fade out moves to the right. Locked clips are not split.
 * `newId` makes the ids for the right parts.
 */
export function splitClips(doc: SessionDoc, ids: Iterable<string>, at: number, newId: () => string): Op | null {
  const media = mediaOf(doc);
  const rate = doc.sample_rate;
  const changes: ClipChange[] = [];
  const added: [string, ClipDoc][] = [];
  for (const { clip, track } of findClips(doc, ids)) {
    if (clip.locked) continue;
    const end = clipEnd(clip, media, rate);
    if (at <= clip.start || at >= end) continue;
    const m = media.get(clip.media_id);
    const leftLen = at - clip.start;
    const rightLen = end - at;
    if (!clip.looped) {
      const cut = clip.source_in + toSource(leftLen, m, rate, stretchOf(clip));
      if (cut <= clip.source_in || cut >= clip.source_out) continue;
    }
    changes.push({
      id: clip.id,
      ...cutRight(clip, m, rate, leftLen),
      fade_in: { ...clip.fade_in, length: Math.min(clip.fade_in.length, leftLen) },
      fade_out: { ...clip.fade_out, length: 0 },
      ...sliceOf(clip, 0, leftLen),
    });
    const right: ClipDoc = {
      ...clip,
      ...cutLeft(clip, m, rate, leftLen),
      id: newId(),
      start: at,
      fade_in: { ...clip.fade_in, length: 0 },
      fade_out: { ...clip.fade_out, length: Math.min(clip.fade_out.length, rightLen) },
    };
    added.push([track.id, withSlices(right, sliceOf(clip, leftLen, leftLen + rightLen))]);
  }
  if (changes.length === 0) return null;
  return { op: "batch", ops: [{ op: "set_clips", changes }, { op: "restore_clips", clips: added }] };
}

/**
 * Clip > Loop Clip for the given clips (locked ones are skipped). Turns
 * looping on when any of them is not looped yet: each keeps its length
 * and can then be trimmed past the end of its audio. Turning it off
 * plays the region once from where the loop started, so nothing jumps.
 */
export function loopChanges(doc: SessionDoc, ids: Iterable<string>): { changes: ClipChange[]; on: boolean } {
  const media = mediaOf(doc);
  const rate = doc.sample_rate;
  // A stretched clip cannot loop: remove the stretch first.
  const found = findClips(doc, ids).filter((f) => !f.clip.locked && !f.clip.stretch);
  const on = !found.every((f) => f.clip.looped);
  const changes: ClipChange[] = [];
  for (const { clip } of found) {
    const m = media.get(clip.media_id);
    if (on) {
      if (!clip.looped) changes.push({ id: clip.id, looped: { length: clipLength(clip, m, rate), phase: 0 } });
      continue;
    }
    const l = clip.looped!;
    const si = clip.source_in + l.phase;
    const out = si + Math.max(1, Math.min(clip.source_out - si, toSource(l.length, m, rate)));
    const sr = m?.sample_rate || rate;
    const len = Math.round(((out - si) * rate) / sr);
    changes.push({
      id: clip.id,
      unloop: true,
      source_in: si,
      source_out: out,
      ...(clip.fade_in.length > len ? { fade_in: { ...clip.fade_in, length: len } } : {}),
      ...(clip.fade_out.length > len ? { fade_out: { ...clip.fade_out, length: len } } : {}),
    });
  }
  return { changes, on };
}

/**
 * The piece of `clip` from `from` to `to` (session samples from its
 * start) as a clip of its own starting at `start`, keeping its place in
 * the source (or loop), its envelopes, and the fades that fall inside.
 */
export function pieceOf(clip: ClipDoc, m: MediaRefDoc | undefined, rate: number, from: number, to: number, start: number, id: string): ClipDoc {
  const len = clipLength(clip, m, rate);
  let p: ClipDoc = { ...clip, id, start };
  if (from > 0) p = { ...p, ...cutLeft(clip, m, rate, from) };
  if (to < len) p = { ...p, ...cutRight(p, m, rate, to - from) };
  p = {
    ...p,
    fade_in: { ...clip.fade_in, length: from === 0 ? Math.min(clip.fade_in.length, to - from) : 0 },
    fade_out: { ...clip.fade_out, length: to === len ? Math.min(clip.fade_out.length, to - from) : 0 },
  };
  // Envelope ids follow the clip id, as in the core.
  const env = (p.envelopes ?? []).filter((e) => e.target.type !== "volume" && e.target.type !== "pan");
  return withSlices({ ...p, envelopes: env }, sliceOf(clip, from, to));
}

/**
 * Splits clips on the given tracks so the range a..b is covered by
 * whole pieces. Returns the ops that do it (one batch, or null when no
 * split is needed) and the ids of every piece inside the range
 * afterwards. Locked clips that need a split throw LockedError.
 */
export function isolate(
  doc: SessionDoc,
  trackIds: Iterable<string>,
  a: number,
  b: number,
  newId: () => string,
): { ops: Op[]; inside: string[] } {
  const media = mediaOf(doc);
  const rate = doc.sample_rate;
  const which = new Set(trackIds);
  const removed: string[] = [];
  const added: [string, ClipDoc][] = [];
  const inside: string[] = [];
  for (const t of doc.tracks) {
    if (!which.has(t.id)) continue;
    for (const c of t.clips) {
      const end = clipEnd(c, media, rate);
      if (end <= a || c.start >= b) continue;
      if (c.start >= a && end <= b) {
        inside.push(c.id);
        continue;
      }
      if (c.locked) throw new LockedError();
      const m = media.get(c.media_id);
      const len = end - c.start;
      // Cut points inside the clip, then one piece per stretch between them.
      const cuts = [0, ...[a - c.start, b - c.start].filter((x) => x > 0 && x < len), len];
      removed.push(c.id);
      for (let i = 0; i + 1 < cuts.length; i++) {
        const from = cuts[i]!;
        const to = cuts[i + 1]!;
        const id = i === 0 ? c.id : newId();
        added.push([t.id, pieceOf(c, m, rate, from, to, c.start + from, id)]);
        if (c.start + from >= a && c.start + to <= b) inside.push(id);
      }
    }
  }
  const ops: Op[] = removed.length ? [{ op: "remove_clips", ids: removed }, { op: "restore_clips", clips: added }] : [];
  return { ops, inside };
}

/** Clips under `at`: the given ids if any span it, otherwise every clip that does. */
export function clipsAt(doc: SessionDoc, at: number, preferred: ReadonlySet<string>): string[] {
  const media = mediaOf(doc);
  const spans = (c: ClipDoc) => at > c.start && at < clipEnd(c, media, doc.sample_rate);
  const all = doc.tracks.flatMap((t) => t.clips.filter(spans).map((c) => c.id));
  const chosen = all.filter((id) => preferred.has(id));
  return chosen.length > 0 ? chosen : all;
}

/** Trims each clip's start or end to `at`. Clips that do not span `at` are left alone. */
export function trimToPlayhead(doc: SessionDoc, ids: Iterable<string>, at: number, edge: "start" | "end"): ClipChange[] {
  const media = mediaOf(doc);
  const out: ClipChange[] = [];
  for (const { clip } of findClips(doc, ids)) {
    const end = clipEnd(clip, media, doc.sample_rate);
    if (at <= clip.start || at >= end) continue;
    const ch = edge === "start" ? trimClip(doc, clip, "left", at - clip.start) : trimClip(doc, clip, "right", at - end);
    if (ch) out.push(ch);
  }
  return out;
}

/** The session's tracks as they would look after `changes`, for drawing a drag in progress. */
export function previewTracks(doc: SessionDoc, changes: readonly ClipChange[]): TrackDoc[] {
  if (changes.length === 0) return doc.tracks;
  const byId = new Map(changes.map((c) => [c.id, c]));
  const moved = new Map<string, ClipDoc[]>();
  const tracks = doc.tracks.map((t) => {
    const clips: ClipDoc[] = [];
    for (const c of t.clips) {
      const ch = byId.get(c.id);
      if (!ch) {
        clips.push(c);
        continue;
      }
      const { id: _id, track_id, volume, pan, unloop, unstretch, clear_audio_type, ...rest } = ch;
      let next = { ...c, ...rest } as ClipDoc;
      if (unloop) next = { ...next, looped: null };
      if (unstretch) next = { ...next, stretch: null };
      if (clear_audio_type) next = { ...next, audio_type: null };
      if (volume) next = withEnvelope(next, "volume", volume);
      if (pan) next = withEnvelope(next, "pan", pan);
      if (track_id && track_id !== t.id) moved.set(track_id, [...(moved.get(track_id) ?? []), next]);
      else clips.push(next);
    }
    return { ...t, clips };
  });
  return tracks.map((t) => (moved.has(t.id) ? { ...t, clips: [...t.clips, ...moved.get(t.id)!] } : t));
}

export class LockedError extends Error {
  constructor() {
    super("A locked clip is in the way. Unlock it first (Clip > Lock Position).");
  }
}

/**
 * Removes the time from `a` to `b` on the given tracks and closes the
 * gap: clips inside are deleted, clips crossing an edge are trimmed,
 * a clip spanning the whole range is split around it, and everything
 * after moves left. Locked clips that would change throw LockedError.
 */
export function rippleDeleteRange(doc: SessionDoc, trackIds: Iterable<string>, a: number, b: number, newId: () => string): Op | null {
  if (b <= a) return null;
  const media = mediaOf(doc);
  const rate = doc.sample_rate;
  const span = b - a;
  const which = new Set(trackIds);
  const changes: ClipChange[] = [];
  const removed: string[] = [];
  const added: [string, ClipDoc][] = [];
  for (const t of doc.tracks) {
    if (!which.has(t.id)) continue;
    for (const c of t.clips) {
      const end = clipEnd(c, media, rate);
      if (end <= a) continue;
      if (c.locked) throw new LockedError();
      const m = media.get(c.media_id);
      if (c.start >= b) {
        changes.push({ id: c.id, start: c.start - span });
      } else if (c.start >= a && end <= b) {
        removed.push(c.id);
      } else if (c.start < a && end <= b) {
        // Crosses the left edge: keep what comes before a.
        changes.push({
          id: c.id,
          ...cutRight(c, m, rate, a - c.start),
          fade_out: { ...c.fade_out, length: Math.min(c.fade_out.length, a - c.start) },
        });
      } else if (c.start >= a) {
        // Crosses the right edge: keep what comes after b, starting at a.
        changes.push({
          id: c.id,
          start: a,
          ...cutLeft(c, m, rate, b - c.start),
          fade_in: { ...c.fade_in, length: Math.min(c.fade_in.length, end - b) },
          ...sliceOf(c, b - c.start, end - c.start),
        });
      } else {
        // Spans the whole range: split around it.
        changes.push({
          id: c.id,
          ...cutRight(c, m, rate, a - c.start),
          fade_out: { ...c.fade_out, length: 0 },
        });
        const piece: ClipDoc = {
          ...c,
          ...cutLeft(c, m, rate, b - c.start),
          id: newId(),
          start: a,
          fade_in: { ...c.fade_in, length: 0 },
          fade_out: { ...c.fade_out, length: Math.min(c.fade_out.length, end - b) },
        };
        added.push([t.id, withSlices(piece, sliceOf(c, b - c.start, end - c.start))]);
      }
    }
  }
  const ops: Op[] = [];
  if (removed.length) ops.push({ op: "remove_clips", ids: removed });
  if (changes.length) ops.push({ op: "set_clips", changes });
  if (added.length) ops.push({ op: "restore_clips", clips: added });
  return ops.length ? { op: "batch", ops } : null;
}

/**
 * Deletes clips and closes the space they took on their tracks: each
 * later clip on the same track moves left by the length of the deleted
 * clips before it.
 */
export function rippleDeleteClips(doc: SessionDoc, ids: Iterable<string>): Op | null {
  const media = mediaOf(doc);
  const rate = doc.sample_rate;
  const gone = new Set(ids);
  const changes: ClipChange[] = [];
  for (const t of doc.tracks) {
    const cuts = t.clips.filter((c) => gone.has(c.id)).map((c) => ({ start: c.start, len: clipEnd(c, media, rate) - c.start }));
    if (cuts.length === 0) continue;
    for (const c of t.clips) {
      if (gone.has(c.id)) continue;
      const shift = cuts.filter((x) => x.start + x.len <= c.start).reduce((n, x) => n + x.len, 0);
      if (shift === 0) continue;
      if (c.locked) throw new LockedError();
      changes.push({ id: c.id, start: Math.max(0, c.start - shift) });
    }
  }
  const removed = [...gone];
  if (removed.length === 0) return null;
  const ops: Op[] = [{ op: "remove_clips", ids: removed }];
  if (changes.length) ops.push({ op: "set_clips", changes });
  return { op: "batch", ops };
}

/** The empty span around `at` on a track, between the clips on either side. Null if `at` is inside a clip or past the last one. */
export function gapAt(doc: SessionDoc, trackId: string, at: number): { start: number; end: number } | null {
  const t = doc.tracks.find((x) => x.id === trackId);
  if (!t) return null;
  const media = mediaOf(doc);
  let start = 0;
  let end = Infinity;
  for (const c of t.clips) {
    const e = clipEnd(c, media, doc.sample_rate);
    if (at >= c.start && at < e) return null;
    if (e <= at) start = Math.max(start, e);
    if (c.start > at) end = Math.min(end, c.start);
  }
  return end === Infinity || end <= start ? null : { start, end };
}

/** Trims clips so they fit inside `a` to `b`. Clips outside the range are left alone. */
export function trimToRange(doc: SessionDoc, ids: Iterable<string>, a: number, b: number): ClipChange[] {
  const media = mediaOf(doc);
  const out: ClipChange[] = [];
  for (const { clip } of findClips(doc, ids)) {
    const end = clipEnd(clip, media, doc.sample_rate);
    if (end <= a || clip.start >= b) continue;
    let merged: ClipChange = { id: clip.id };
    if (clip.start < a) {
      const ch = trimClip(doc, clip, "left", a - clip.start);
      if (ch) merged = { ...merged, ...ch };
    }
    if (end > b) {
      const ch = trimClip(doc, clip, "right", b - end);
      if (ch) merged = { ...merged, ...ch };
    }
    if (Object.keys(merged).length > 1) out.push(merged);
  }
  return out;
}

/** Clips copied with Edit > Copy, positioned relative to the earliest one. */
export interface ClipBoard {
  /** Row offset from the topmost copied track, and the clip with `start` relative to the first clip. */
  items: { row: number; clip: ClipDoc }[];
}

export function copyClips(doc: SessionDoc, ids: Iterable<string>): ClipBoard | null {
  const found = findClips(doc, ids);
  if (found.length === 0) return null;
  const top = Math.min(...found.map((f) => f.trackIndex));
  const first = Math.min(...found.map((f) => f.clip.start));
  return { items: found.map((f) => ({ row: f.trackIndex - top, clip: { ...f.clip, start: f.clip.start - first, locked: false } })) };
}

/**
 * Pastes copied clips at `at` with the topmost one on track `row`.
 * Rows past the last track land on the last track.
 */
export function pasteClips(doc: SessionDoc, board: ClipBoard, at: number, row: number, newId: () => string): { op: Op; ids: string[] } | null {
  if (doc.tracks.length === 0 || board.items.length === 0) return null;
  const media = new Set(doc.media.map((m) => m.id));
  const clips: [string, ClipDoc][] = [];
  // Pasted copies keep their grouping among themselves, apart from the originals.
  const groups = new Map<string, string>();
  for (const it of board.items) {
    if (!media.has(it.clip.media_id)) continue;
    const t = doc.tracks[Math.min(doc.tracks.length - 1, Math.max(0, row + it.row))]!;
    const g = it.clip.group;
    if (g && !groups.has(g)) groups.set(g, newId());
    clips.push([t.id, { ...it.clip, id: newId(), start: Math.max(0, Math.round(at + it.clip.start)), group: g ? groups.get(g)! : null }]);
  }
  if (clips.length === 0) return null;
  return { op: { op: "restore_clips", clips }, ids: clips.map(([, c]) => c.id) };
}

/**
 * Automatic crossfades: after `changes`, wherever a changed clip
 * overlaps another clip on the same track, the earlier clip fades out
 * and the later one fades in across the overlap. Returns `changes` with
 * the fades merged in.
 */
export function withCrossfades(doc: SessionDoc, changes: ClipChange[]): ClipChange[] {
  if (changes.length === 0) return changes;
  const media = mediaOf(doc);
  const rate = doc.sample_rate;
  const touched = new Set(changes.map((c) => c.id));
  const merged = new Map(changes.map((c) => [c.id, { ...c }]));
  const setFade = (c: ClipDoc, which: "fade_in" | "fade_out", length: number) => {
    const cur = merged.get(c.id) ?? { id: c.id };
    const base = cur[which] ?? c[which];
    if (base.length === length) return;
    merged.set(c.id, { ...cur, [which]: { ...base, length, shape: base.length === 0 ? settings.current.fadeShape : base.shape } });
  };
  for (const t of previewTracks(doc, changes)) {
    const clips = [...t.clips].sort((a, b) => a.start - b.start);
    for (let i = 0; i < clips.length; i++) {
      for (let j = i + 1; j < clips.length; j++) {
        const a = clips[i]!;
        const b = clips[j]!;
        if (!touched.has(a.id) && !touched.has(b.id)) continue;
        const aEnd = clipEnd(a, media, rate);
        if (b.start >= aEnd) break;
        const bEnd = clipEnd(b, media, rate);
        // A clip fully inside another is layered, not crossfaded.
        if (bEnd <= aEnd) continue;
        const overlap = aEnd - b.start;
        setFade(a, "fade_out", overlap);
        setFade(b, "fade_in", overlap);
      }
    }
  }
  return [...merged.values()];
}

/** The ids plus every clip that shares a group with one of them. */
export function withGroups(doc: SessionDoc, ids: Iterable<string>): string[] {
  const want = new Set(ids);
  const groups = new Set<string>();
  for (const t of doc.tracks) for (const c of t.clips) if (want.has(c.id) && c.group) groups.add(c.group);
  if (groups.size === 0) return [...want];
  for (const t of doc.tracks) for (const c of t.clips) if (c.group && groups.has(c.group)) want.add(c.id);
  return [...want];
}

/**
 * Inserts `len` session samples of silence at `at` on the given tracks:
 * clips that start at or after `at` move right, and a clip spanning `at`
 * is split there with its right part moved. Markers after `at` move too
 * when every track is included, so they stay with their audio. Locked
 * clips that would move throw LockedError.
 */
export function insertTime(doc: SessionDoc, trackIds: Iterable<string>, at: number, len: number, newId: () => string): Op | null {
  if (len <= 0) return null;
  const which = new Set(trackIds);
  const media = mediaOf(doc);
  const rate = doc.sample_rate;
  const ops: Op[] = [];
  // Split clips that span the insert point first, so every piece after it moves.
  const spanning = doc.tracks
    .filter((t) => which.has(t.id))
    .flatMap((t) => t.clips.filter((c) => c.start < at && clipEnd(c, media, rate) > at));
  if (spanning.some((c) => c.locked)) throw new LockedError();
  let cur = doc;
  if (spanning.length) {
    const split = splitClips(doc, spanning.map((c) => c.id), at, newId);
    if (split) {
      ops.push(split);
      // The split's new right-hand pieces start exactly at `at`.
      const added = split.op === "batch" ? split.ops.flatMap((o) => (o.op === "restore_clips" ? o.clips : [])) : [];
      cur = {
        ...doc,
        tracks: doc.tracks.map((t) => {
          const mine = added.filter(([tid]) => tid === t.id).map(([, c]) => c);
          const changes = split.op === "batch" ? split.ops.flatMap((o) => (o.op === "set_clips" ? o.changes : [])) : [];
          const clips = t.clips.map((c) => {
            const ch = changes.find((x) => x.id === c.id);
            return ch ? ({ ...c, ...ch } as ClipDoc) : c;
          });
          return { ...t, clips: [...clips, ...mine] };
        }),
      };
    }
  }
  const changes: ClipChange[] = [];
  for (const t of cur.tracks) {
    if (!which.has(t.id)) continue;
    for (const c of t.clips) {
      if (c.start < at) continue;
      if (c.locked) throw new LockedError();
      changes.push({ id: c.id, start: c.start + len });
    }
  }
  if (changes.length) ops.push({ op: "set_clips", changes });
  const all = doc.tracks.every((t) => which.has(t.id));
  if (all) {
    const mk = doc.markers.filter((m) => m.position >= at).map((m) => ({ id: m.id, position: m.position + len }));
    if (mk.length) ops.push({ op: "set_markers", changes: mk });
  }
  return ops.length ? { op: "batch", ops } : null;
}

/**
 * Silences the time from `a` to `b` on the given tracks without moving
 * anything: clips are split at both edges and the pieces inside are
 * removed, leaving a gap of exactly that length. Locked clips that would
 * change throw LockedError.
 */
export function clearRange(doc: SessionDoc, trackIds: Iterable<string>, a: number, b: number, newId: () => string): Op | null {
  if (b <= a) return null;
  const { ops, inside } = isolate(doc, trackIds, a, b, newId);
  if (inside.length === 0) return null;
  return { op: "batch", ops: [...ops, { op: "remove_clips", ids: inside }] };
}

/**
 * Alt+drag copies: the edit that puts copies of the moved clips where the
 * drag left them (new ids, on their new tracks), leaving the originals
 * in place. Changes to other clips (crossfades made by the drop) still
 * apply. Copies that shared a group share a new one of their own.
 */
export function copyMoved(doc: SessionDoc, moving: readonly string[], changes: readonly ClipChange[], newId: () => string): { op: Op; ids: string[] } {
  const moved = new Set(moving);
  const copies: [string, ClipDoc][] = [];
  const others: ClipChange[] = [];
  const groups = new Map<string, string>();
  const ids: string[] = [];
  for (const ch of changes) {
    if (!moved.has(ch.id)) {
      others.push(ch);
      continue;
    }
    const track = doc.tracks.find((t) => t.clips.some((c) => c.id === ch.id));
    const clip = track?.clips.find((c) => c.id === ch.id);
    if (!track || !clip) continue;
    const { id: _id, track_id, ungroup: _ungroup, group: _group, ...fields } = ch;
    const id = newId();
    let group: string | null = null;
    if (clip.group) {
      group = groups.get(clip.group) ?? newId();
      groups.set(clip.group, group);
    }
    copies.push([track_id ?? track.id, { ...clip, ...fields, id, group } as ClipDoc]);
    ids.push(id);
  }
  const ops: Op[] = [{ op: "restore_clips", clips: copies }];
  if (others.length) ops.push({ op: "set_clips", changes: others });
  return { op: ops.length === 1 ? ops[0]! : { op: "batch", ops }, ids };
}
