// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * The math of the Multitrack clip drags: what a slip, an edge stretch, an
 * edge trim or a fade handle drag changes, and the undo label a finished
 * drag gets. The editor turns pointer moves into the distances used here.
 */

import { clipLength, stretchOf, type ClipChange, type ClipDoc, type MediaRefDoc, type SessionDoc } from "../../stores/session";
import { fadeClip, MIN_CLIP, nearest, trimClip } from "./clip-edit";
import type { Gesture } from "./lane-hit";
import { stretchTo } from "./stretch";

/**
 * Slip: the audio inside a clip moves by `dt` session samples, the clip
 * stays put. Dragging right brings earlier audio into the clip.
 */
export function slipChange(clip: ClipDoc, m: MediaRefDoc | undefined, rate: number, dt: number): ClipChange {
  const sr = m?.sample_rate || rate;
  const d = Math.round((dt * sr) / rate / stretchOf(clip));
  if (clip.looped) {
    const region = Math.max(1, clip.source_out - clip.source_in);
    const phase = (((clip.looped.phase - d) % region) + region) % region;
    return { id: clip.id, looped: { ...clip.looped, phase } };
  }
  const room = m && m.length > 0 ? m.length - clip.source_out : Infinity;
  const shift = Math.max(-clip.source_in, Math.min(room, -d));
  return { id: clip.id, source_in: clip.source_in + shift, source_out: clip.source_out + shift };
}

/**
 * An edge dragged `dx` samples with stretching on: the clip takes the new
 * length by stretching its audio. `end` is where the clip ended before.
 */
export function stretchDrag(
  clip: ClipDoc,
  m: MediaRefDoc | undefined,
  rate: number,
  side: "left" | "right",
  end: number,
  dx: number,
  targets: number[],
  tol: number,
): { changes: ClipChange[]; hint: string | null } {
  let edge = (side === "left" ? clip.start : end) + dx;
  if (tol > 0) edge = nearest(edge, targets, tol) ?? edge;
  edge = Math.round(edge);
  const left = side === "left" ? Math.max(0, Math.min(edge, end - MIN_CLIP)) : clip.start;
  const right = side === "right" ? Math.max(clip.start + MIN_CLIP, edge) : end;
  const r = stretchTo(clip, m, rate, right - left, clip.stretch?.keep_pitch ?? true);
  return {
    changes: r ? [left !== clip.start ? { ...r.change, start: left } : r.change] : [],
    hint: r ? `Stretch ${Math.round(r.ratio * 1000) / 10}%${r.limited ? " (limit)" : ""}` : null,
  };
}

/**
 * An edge trim of `clip` by `dx` samples. The grabbed clip snaps; the
 * other clips in `ids` follow by the same amount, each within its own limits.
 */
export function trimDrag(
  d0: SessionDoc,
  clip: ClipDoc,
  side: "left" | "right",
  dx: number,
  targets: number[],
  tol: number,
  ids: readonly string[],
  m: MediaRefDoc | undefined,
  rate: number,
  end: number,
): ClipChange[] {
  const c = trimClip(d0, clip, side, dx, targets, tol);
  const changes = c ? [c] : [];
  if (c && ids.length > 1) {
    const moved = { ...clip, ...c } as ClipDoc;
    const by = side === "left" ? moved.start - clip.start : moved.start + clipLength(moved, m, rate) - end;
    for (const t of d0.tracks)
      for (const other of t.clips) {
        if (other.id === clip.id || !ids.includes(other.id)) continue;
        const o = trimClip(d0, other, side, by);
        if (o) changes.push(o);
      }
  }
  return changes;
}

/**
 * A fade handle drag. Across sets the fade's length (to session sample
 * `at`, once the pointer has moved `across`); up and down (`dy` pixels)
 * bends its curve, -100 to 100. `tip` is the readout beside the pointer.
 */
export function fadeDrag(
  d0: SessionDoc,
  clip: ClipDoc,
  which: "in" | "out",
  at: number,
  end: number,
  across: boolean,
  dy: number,
  rate: number,
): { changes: ClipChange[]; tip: string } {
  const was = which === "in" ? clip.fade_in : clip.fade_out;
  const bend = Math.max(-1, Math.min(1, Math.round(((was.curve ?? 0) - dy / 80) * 100) / 100));
  const c = across ? fadeClip(d0, clip, which, which === "in" ? at - clip.start : end - at) : null;
  const fade = { ...was, ...(c ? (which === "in" ? c.fade_in : c.fade_out) : {}), curve: Math.abs(bend) < 0.02 ? 0 : bend };
  // Dragged all the way back, the fade is removed (length 0), not left as it was.
  const changes = fade.length > 0 || was.length > 0 ? [which === "in" ? { id: clip.id, fade_in: fade } : { id: clip.id, fade_out: fade }] : [];
  const skew = Math.round(fade.curve * 100);
  const tip = `${which === "in" ? "Fade in" : "Fade out"} ${(fade.length / rate).toFixed(2)} s, ${fade.shape === "cosine" ? "curve" : "straight"} ${skew > 0 ? "+" : ""}${skew}`;
  return { changes, tip };
}

/** The undo label of a finished clip drag. `count` is how many clips the drag took. */
export function gestureLabel(kind: Gesture, stretching: boolean, count: number, changes: readonly ClipChange[]): string {
  return kind === "gain"
    ? "Clip Level"
    : stretching
      ? "Stretch Clip"
      : kind === "move"
        ? count > 1
          ? `Move ${count} Clips`
          : "Move Clip"
        : kind === "fade-in"
          ? changes[0]?.fade_in?.length === 0
            ? "Remove Fade In"
            : "Fade In"
          : kind === "fade-out"
            ? changes[0]?.fade_out?.length === 0
              ? "Remove Fade Out"
              : "Fade Out"
            : changes.length > 1
              ? `Trim ${changes.length} Clips`
              : "Trim Clip";
}
