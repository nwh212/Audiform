// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Music Fit, interface side: turns the core's plan (which parts of the
 * music to play, in order) into clips on the same track, overlapped by
 * a short crossfade at each join and grouped so they move together. One
 * undoable edit; the original clip comes back on undo.
 */

import type { ClipDoc, MediaRefDoc, Op, SessionDoc } from "../../stores/session";

export interface FitSegment {
  /** Seconds into the file. */
  start: number;
  end: number;
}

export interface FitPlan {
  segments: FitSegment[];
  length: number;
  stretch: number;
  joins: number;
  quality: number;
  tempo: number;
}

/** Crossfade at each join, in seconds. Long enough to hide the cut, short enough to stay on the beat. */
export const FIT_XFADE = 0.04;

/** The edit that replaces `clip` with the plan's segments. */
export function fitOps(doc: SessionDoc, trackId: string, clip: ClipDoc, media: MediaRefDoc, plan: FitPlan, newId: () => string): Op {
  const sess = doc.sample_rate;
  const src = media.sample_rate || sess;
  const stretch = Math.abs(plan.stretch - 1) < 0.0005 ? 1 : plan.stretch;
  const xf = Math.round(FIT_XFADE * sess * stretch);
  const group = plan.segments.length > 1 ? newId() : (clip.group ?? null);
  let start = clip.start;
  const clips: [string, ClipDoc][] = plan.segments.map((seg, k) => {
    const sourceIn = Math.max(0, Math.round(seg.start * src));
    const sourceOut = Math.min(media.length || Number.MAX_SAFE_INTEGER, Math.round(seg.end * src));
    const length = Math.round((((sourceOut - sourceIn) * sess) / src) * stretch);
    const first = k === 0;
    const last = k === plan.segments.length - 1;
    const c: ClipDoc = {
      ...clip,
      id: first && plan.segments.length === 1 ? clip.id : newId(),
      start,
      source_in: sourceIn,
      source_out: sourceOut,
      fade_in: first ? clip.fade_in : { length: xf, shape: "cosine", curve: 0 },
      fade_out: last ? clip.fade_out : { length: xf, shape: "cosine", curve: 0 },
      looped: null,
      stretch: stretch === 1 ? null : { ratio: stretch, keep_pitch: true },
      group,
      // Envelopes were drawn for the old length; the new clips start clean.
      envelopes: [],
    };
    start += length - xf;
    return [trackId, c];
  });
  return { op: "batch", ops: [{ op: "remove_clips", ids: [clip.id] }, { op: "restore_clips", clips }] };
}

/** One line for the dialog: how the plan sounds. */
export function describePlan(p: FitPlan): string {
  const joins = p.joins === 0 ? "No joins" : p.joins === 1 ? "1 join" : `${p.joins} joins`;
  const match = `${Math.round(p.quality * 100)}% match`;
  const st = Math.abs(p.stretch - 1) < 0.0005 ? "no stretch" : `stretched ${((p.stretch - 1) * 100).toFixed(1)}%`;
  return `${joins}, ${match}, ${Math.round(p.tempo)} BPM, ${st}`;
}

const clock = (s: number) => {
  const m = Math.floor(s / 60);
  const rest = (s - m * 60).toFixed(1);
  return m > 0 ? `${m}:${rest.padStart(4, "0")}` : `${rest} s`;
};

/**
 * Where the plan joins, in seconds into the clip (`origin` is where the
 * clip starts in its file): "Cuts 0:42.0 to 1:10.0" for a skip,
 * "Repeats 0:30.0 to 0:46.0, 2 more times" for a loop.
 */
export function describeJoins(p: FitPlan, origin: number): string {
  const [a, b] = p.segments;
  if (!a || !b) return "Plays the whole clip";
  // Each side runs half a crossfade past the beat it joins on.
  const out = a.end - FIT_XFADE / 2 - origin;
  const back = b.start + FIT_XFADE / 2 - origin;
  if (back > out) return `Cuts ${clock(out)} to ${clock(back)}`;
  const more = p.segments.length - 1;
  return `Repeats ${clock(back)} to ${clock(out)}, ${more === 1 ? "1 more time" : `${more} more times`}`;
}
