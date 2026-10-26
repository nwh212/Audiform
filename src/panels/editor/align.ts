// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Clip > Phase Align, interface side. With two clips selected on two
 * tracks, the one higher up is the reference and stays put; the other
 * moves by what the core measured, and its track's polarity flips when
 * the best match is upside down. One undoable edit.
 */

import type { Op, SessionDoc } from "../../stores/session";

export interface Alignment {
  /** Samples the moving clip trails the reference by (negative: early). */
  lag: number;
  /** 0 to 1. */
  score: number;
  inverted: boolean;
}

/** Search either way, in ms: covers mics across a room, and a camera's audio a little out. */
export const ALIGN_MAX_MS = 100;

/** [reference, moving] clip ids and the moving clip's track, or null unless two clips on two tracks are selected. */
export function alignPair(doc: SessionDoc, selected: Iterable<string>): { reference: string; other: string; trackId: string } | null {
  const ids = [...selected];
  if (ids.length !== 2) return null;
  const at = ids.map((id) => {
    const t = doc.tracks.findIndex((x) => x.clips.some((c) => c.id === id));
    return { id, t, clip: doc.tracks[t]?.clips.find((c) => c.id === id) };
  });
  if (at.some((x) => x.t < 0) || at[0]!.t === at[1]!.t) return null;
  if (at.some((x) => x.clip?.locked)) return null;
  const [ref, mov] = at[0]!.t < at[1]!.t ? [at[0]!, at[1]!] : [at[1]!, at[0]!];
  return { reference: ref.id, other: mov.id, trackId: doc.tracks[mov.t]!.id };
}

/** The edit that applies `a` to the moving clip, or null when it is already in line. */
export function alignOps(doc: SessionDoc, other: string, trackId: string, a: Alignment): Op | null {
  const clip = doc.tracks.find((t) => t.id === trackId)?.clips.find((c) => c.id === other);
  const track = doc.tracks.find((t) => t.id === trackId);
  if (!clip || !track) return null;
  const ops: Op[] = [];
  const start = Math.max(0, clip.start - a.lag);
  if (start !== clip.start) ops.push({ op: "set_clips", changes: [{ id: other, start }] });
  if (a.inverted) ops.push({ op: "set_track", id: trackId, changes: { polarity_inverted: !track.polarity_inverted } });
  if (ops.length === 0) return null;
  return ops.length === 1 ? ops[0]! : { op: "batch", ops };
}

/** One line for the notice. */
export function describeAlign(a: Alignment, rate: number, name: string, trackName: string): string {
  const ms = Math.abs(a.lag / rate) * 1000;
  const move = a.lag === 0 ? `${name} was already in line` : `Moved ${name} ${ms.toFixed(2)} ms ${a.lag > 0 ? "earlier" : "later"}`;
  const flip = a.inverted ? `, and flipped the polarity of ${trackName}` : "";
  const weak = a.score < 0.3 ? " The match is weak: these may not be the same sound." : "";
  return `${move}${flip} (${Math.round(a.score * 100)}% match).${weak}`;
}
