// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Hit-testing in the Multitrack lanes: which clip, and which part of it
 * (edge, fade handle, level handle, envelope), sits under the pointer.
 * Also the cursors each part shows. The sizes match LaneCanvas.
 */

import type { EditorState } from "../../stores/editor";
import { clipLength, type ClipDoc, type MediaRefDoc, type TrackDoc } from "../../stores/session";
import { gainBox } from "./clip-gain";
import { chipBox, chipLabel, fxLaneOf, levelAt, pointsOf, rangeOf, restValue, valueToY, type EnvKind } from "./envelope";
import { sampleAt, trackHeight, xOf, TRACK_GAP, type Viewport } from "./timeline";

export const EDGE_PX = 6;
export const TITLE_H = 15;
export const SNAP_PX = 8;
/** Stretch handles: triangles this many pixels wide in a clip's top corners (see LaneCanvas). */
export const STRETCH_PX = 11;

export type Gesture = "move" | "trim-left" | "trim-right" | "stretch-left" | "stretch-right" | "fade-in" | "fade-out" | "env-line" | "env-point" | "gain";

/** A razor blade for the Razor tool; the hot spot is the blade's cutting point. */
export const RAZOR_CURSOR = `url("data:image/svg+xml;utf8,${encodeURIComponent(
  '<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"><g transform="rotate(-35 12 12)"><rect x="9" y="1" width="6" height="15" rx="1" fill="%23e8e8ee" stroke="%23111" stroke-width="1"/><path d="M12 3v11" stroke="%23111" stroke-width="1" stroke-dasharray="2 1.5"/><rect x="10" y="16" width="4" height="6" rx="1" fill="%23e5534b" stroke="%23111" stroke-width="1"/></g></svg>',
).replace(/%2523/g, "%23")}") 4 22, crosshair`;

/** Edge trim cursors: a double arrow (the edge moves either way) beside a bracket on the clip's side. */
const trimCursor = (left: boolean) =>
  `url("data:image/svg+xml;utf8,${encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 22 22"><g transform="${left ? "" : "translate(22 0) scale(-1 1)"}" stroke-linecap="square" fill="none"><path d="M14 4h3v14h-3" stroke="#111" stroke-width="3.6"/><path d="M14 4h3v14h-3" stroke="#f2f2f5" stroke-width="1.6"/><path d="M2 11h10M5 8l-3 3 3 3M9 8l3 3-3 3" stroke="#111" stroke-width="3.4"/><path d="M2 11h10M5 8l-3 3 3 3M9 8l3 3-3 3" stroke="#f2f2f5" stroke-width="1.4"/></g></svg>`,
  )}") ${left ? 7 : 15} 11, ew-resize`;

export const CURSORS: Record<Gesture, string> = {
  move: "grab",
  "trim-left": trimCursor(true),
  "trim-right": trimCursor(false),
  "stretch-left": "ew-resize",
  "stretch-right": "ew-resize",
  "fade-in": "ew-resize",
  "fade-out": "ew-resize",
  "env-line": "crosshair",
  "env-point": "move",
  gain: "ns-resize",
};

/** The y range the volume envelope uses inside a lane of `height` pixels. Matches LaneCanvas. */
export const envBox = (height: number) => ({ top: 1 + 14 + 2, height: height - TRACK_GAP - 2 - 14 - 4 });

/** What the lanes are drawn with: the view, the files (for clip lengths) and the session rate. */
export interface LaneGeometry {
  view: Viewport;
  media: ReadonlyMap<string, MediaRefDoc>;
  rate: number;
}

/** The editor settings hit-testing depends on. */
export type HitOptions = Pick<EditorState, "stretchHandles" | "fxLanes" | "showVolume" | "showPan" | "showFx">;

export interface Zone {
  kind: Gesture;
  clip: ClipDoc;
  index?: number;
  env?: EnvKind;
}

/** The topmost clip of `track` at lane x, if any. */
export function clipAt(g: LaneGeometry, track: TrackDoc, x: number): ClipDoc | null {
  for (let i = track.clips.length - 1; i >= 0; i--) {
    const c = track.clips[i]!;
    const x0 = xOf(c.start, g.view);
    const x1 = xOf(c.start + clipLength(c, g.media.get(c.media_id), g.rate), g.view);
    if (x >= x0 && x <= x1) return c;
  }
  return null;
}

/** A clip's start or end that lies under a clip drawn above it (an overlap or crossfade). */
export interface HiddenEdge {
  clip: ClipDoc;
  side: "start" | "end";
  /** The edge's lane x. */
  x: number;
  /** Where the clip on top covers it, in lane x: the overlap's other end. */
  to: number;
}

/** Every clip edge in `track` that a later (higher) clip covers. */
export function hiddenEdges(g: LaneGeometry, track: TrackDoc): HiddenEdge[] {
  const spans = track.clips.map((c) => {
    const x0 = xOf(c.start, g.view);
    return { c, x0, x1: xOf(c.start + clipLength(c, g.media.get(c.media_id), g.rate), g.view) };
  });
  const out: HiddenEdge[] = [];
  spans.forEach((a, i) => {
    for (const side of ["start", "end"] as const) {
      const x = side === "start" ? a.x0 : a.x1;
      const over = spans.slice(i + 1).find((b) => x > b.x0 + 0.5 && x < b.x1 - 0.5);
      if (over) out.push({ clip: a.c, side, x, to: side === "start" ? Math.min(a.x1, over.x1) : Math.max(a.x0, over.x0) });
    }
  });
  return out;
}

/** The part of a clip under (x, y) in a lane, and the gesture a press there starts. */
export function zoneAt(g: LaneGeometry, ed: HitOptions, track: TrackDoc, x: number, y: number): Zone | null {
  const view = g.view;
  const clip = clipAt(g, track, x);
  if (!clip) return null;
  // The edge of a clip running under this one, below the fade handles: trim it.
  if (y > TITLE_H + 21) {
    const under = hiddenEdges(g, track).find((e) => e.clip !== clip && !e.clip.locked && Math.abs(e.x - x) <= EDGE_PX);
    if (under) return { kind: under.side === "start" ? "trim-left" : "trim-right", clip: under.clip };
  }
  const x0 = xOf(clip.start, view);
  const x1 = xOf(clip.start + clipLength(clip, g.media.get(clip.media_id), g.rate), view);
  const w = x1 - x0;
  // Stretch handles fill the title bar's corners when they are shown.
  if (ed.stretchHandles && y <= TITLE_H + 1 && w > 2 * STRETCH_PX + 4 && !clip.looped && !clip.locked) {
    if (x - x0 <= STRETCH_PX) return { kind: "stretch-left", clip };
    if (x1 - x <= STRETCH_PX) return { kind: "stretch-right", clip };
  }
  // The level handle in the bottom-left corner (see clip-gain.ts).
  const gb = gainBox(x0, x1, trackHeight(track) - TRACK_GAP - 2, TITLE_H + 1);
  if (gb && !clip.locked && x >= gb.x - 2 && x <= gb.x + gb.w + 2 && y >= gb.y - 2 && y <= gb.y + gb.h + 2) return { kind: "gain", clip };
  // Fade handles sit just under the title bar (see LaneCanvas).
  if (y > TITLE_H && y <= TITLE_H + 21 && w > 30) {
    const hit = (hx: number) => Math.abs(x - Math.min(Math.max(hx, x0 + 8), x1 - 8)) <= 8;
    if (hit(x0 + clip.fade_in.length / view.spp)) return { kind: "fade-in", clip };
    if (hit(x1 - clip.fade_out.length / view.spp)) return { kind: "fade-out", clip };
  }
  // Edges come before the volume and pan lines, which run right up to them.
  if (w > 16 && !clip.locked && y > TITLE_H && x - x0 <= EDGE_PX) return { kind: "trim-left", clip };
  if (w > 16 && !clip.locked && y > TITLE_H && x1 - x <= EDGE_PX) return { kind: "trim-right", clip };
  if (y > TITLE_H && !clip.locked) {
    const box = envBox(trackHeight(track));
    const lane = fxLaneOf(clip, ed.fxLanes);
    const kinds: EnvKind[] = [];
    if (ed.showVolume) kinds.push("volume");
    if (ed.showPan) kinds.push("pan");
    if (ed.showFx && lane) kinds.push("fx");
    // Points win over lines, so a point sitting on the other line stays reachable.
    for (const env of kinds) {
      const pts = pointsOf(clip, env, lane);
      const range = rangeOf(env, clip, lane);
      const hit = pts.findIndex(
        (p) => Math.abs(xOf(clip.start + p.position, view) - x) <= 5 && Math.abs(valueToY(env, p.value, box.top, box.height, range) - y) <= 5,
      );
      if (hit >= 0) return { kind: "env-point", clip, index: hit, env };
    }
    for (const env of kinds) {
      const pts = pointsOf(clip, env, lane);
      const v = pts.length ? levelAt(pts, sampleAt(x, view) - clip.start) : restValue(env, clip, lane);
      const lineY = valueToY(env, v, box.top, box.height, rangeOf(env, clip, lane));
      if (Math.abs(lineY - y) <= 4) return { kind: "env-line", clip, env };
    }
  }
  if (w > 16 && x - x0 <= EDGE_PX) return { kind: "trim-left", clip };
  if (w > 16 && x1 - x <= EDGE_PX) return { kind: "trim-right", clip };
  return { kind: "move", clip };
}

/** The clip whose envelope chip is under (x, y), if the point is on it. */
export function chipAt(g: LaneGeometry, ed: Pick<EditorState, "showVolume" | "showPan" | "showFx">, track: TrackDoc, x: number, y: number): ClipDoc | null {
  if (y > TITLE_H) return null;
  const clip = clipAt(g, track, x);
  if (!clip) return null;
  const x0 = xOf(clip.start, g.view);
  const x1 = xOf(clip.start + clipLength(clip, g.media.get(clip.media_id), g.rate), g.view);
  const box = chipBox(x0, x1, chipLabel(ed.showVolume, ed.showPan, ed.showFx));
  return box && x >= box.x && x <= box.x + box.w ? clip : null;
}
