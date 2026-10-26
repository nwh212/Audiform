// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * 3D Space sessions in the interface: each layout's speakers and where
 * they stand (the same room as the core's `dsp/space.rs`: x left -1 to
 * right 1, y back -1 to front 1, z floor 0 to ceiling 1), and the room
 * view's sums.
 */

import { DEFAULT_SPACE_POS, type SpaceLayout, type SpacePosDoc } from "../../stores/session";

export interface SpeakerSpot {
  name: string;
  x: number;
  y: number;
  z: number;
}

const S = (name: string, x: number, y: number, z: number): SpeakerSpot => ({ name, x, y, z });
const FRONT = [S("L", -1, 1, 0), S("C", 0, 1, 0), S("R", 1, 1, 0)];
const REAR5 = [S("Ls", -1, -1, 0), S("Rs", 1, -1, 0)];
const SIDE7 = [S("Lss", -1, 0, 0), S("Rss", 1, 0, 0), S("Lrs", -1, -1, 0), S("Rrs", 1, -1, 0)];
const TOP2 = [S("Ltm", -1, 0, 1), S("Rtm", 1, 0, 1)];
const TOP4 = [S("Ltf", -1, 1, 1), S("Rtf", 1, 1, 1), S("Ltr", -1, -1, 1), S("Rtr", 1, -1, 1)];

/** The placed speakers of a layout (its low-frequency channel has no place). */
export function speakerSpots(layout: SpaceLayout): SpeakerSpot[] {
  switch (layout) {
    case "2.0":
      return [S("L", -1, 1, 0), S("R", 1, 1, 0)];
    case "5.1":
      return [...FRONT, ...REAR5];
    case "7.1":
      return [...FRONT, ...SIDE7];
    case "5.1.2":
      return [...FRONT, ...REAR5, ...TOP2];
    case "7.1.2":
      return [...FRONT, ...SIDE7, ...TOP2];
    case "5.1.4":
      return [...FRONT, ...REAR5, ...TOP4];
    case "7.1.4":
      return [...FRONT, ...SIDE7, ...TOP4];
  }
}

/** What each layout is, for menus. */
export const LAYOUT_NAMES: Record<SpaceLayout, string> = {
  "2.0": "2.0 Stereo",
  "5.1": "5.1",
  "7.1": "7.1",
  "5.1.2": "5.1.2 (two height speakers)",
  "7.1.2": "7.1.2 (two height speakers)",
  "5.1.4": "5.1.4 (four height speakers)",
  "7.1.4": "7.1.4 (four height speakers)",
};

/** A spot with every value in range (missing parts take the default). */
export function tidyPos(p: Partial<SpacePosDoc> | null | undefined): SpacePosDoc {
  const d = DEFAULT_SPACE_POS;
  const c = (v: number | undefined, lo: number, hi: number, def: number) => (typeof v === "number" && Number.isFinite(v) ? Math.min(hi, Math.max(lo, v)) : def);
  return {
    x: c(p?.x, -1, 1, d.x),
    y: c(p?.y, -1, 1, d.y),
    // Below 0 is below ear level.
    z: c(p?.z, -1, 1, d.z),
    size: c(p?.size, 0, 1, d.size),
    width: c(p?.width, 0, 1, d.width),
    rotation: c(p?.rotation, -180, 180, 0),
    lfe: c(p?.lfe, 0, 1, 0),
    spread: c(p?.spread, 0, 1, 0),
    spatialize: c(p?.spatialize, 0, 1, 0),
  };
}

/** The room view: room units (-1 to 1) to pixels in a square `size` across, front at the top, with a margin. */
export function toView(x: number, y: number, size: number): [number, number] {
  const m = size * 0.1;
  const span = size - 2 * m;
  return [m + ((x + 1) / 2) * span, m + ((1 - y) / 2) * span];
}

/** Pixels in the room view to room units, kept inside the room. */
export function fromView(px: number, py: number, size: number): [number, number] {
  const m = size * 0.1;
  const span = size - 2 * m;
  const x = ((px - m) / span) * 2 - 1;
  const y = 1 - ((py - m) / span) * 2;
  return [Math.min(1, Math.max(-1, x)), Math.min(1, Math.max(-1, y))];
}

/**
 * Where a stereo source's two sides sit, [x, y] each: `width` either side
 * of its spot, the pair turned by its rotation (clockwise seen from above),
 * kept in the room. The core's `dsp/space.rs` works it out the same way.
 */
export function sides(p: SpacePosDoc): [[number, number], [number, number]] {
  const a = ((p.rotation ?? 0) * Math.PI) / 180;
  const dx = p.width * Math.cos(a);
  const dy = -p.width * Math.sin(a);
  const keep = (v: number) => Math.min(1, Math.max(-1, v));
  return [
    [keep(p.x - dx), keep(p.y - dy)],
    [keep(p.x + dx), keep(p.y + dy)],
  ];
}

/** The rotation (degrees) that puts a pair's right side toward (x, y) from its spot. */
export function rotationToward(p: SpacePosDoc, x: number, y: number): number {
  const deg = (Math.atan2(-(y - p.y), x - p.x) * 180) / Math.PI;
  return Math.round(deg);
}

/** Every channel of a layout in file order, the low-frequency one included (as the core's `speakers`). */
export function channelNames(layout: SpaceLayout): string[] {
  const base = ["L", "R", "C", "LFE"];
  switch (layout) {
    case "2.0":
      return ["L", "R"];
    case "5.1":
      return [...base, "Ls", "Rs"];
    case "7.1":
      return [...base, "Lrs", "Rrs", "Lss", "Rss"];
    case "5.1.2":
      return [...base, "Ls", "Rs", "Ltm", "Rtm"];
    case "7.1.2":
      return [...base, "Lrs", "Rrs", "Lss", "Rss", "Ltm", "Rtm"];
    case "5.1.4":
      return [...base, "Ls", "Rs", "Ltf", "Rtf", "Ltr", "Rtr"];
    case "7.1.4":
      return [...base, "Lrs", "Rrs", "Lss", "Rss", "Ltf", "Rtf", "Ltr", "Rtr"];
  }
}

/** Turns degrees into -180 to 180. */
export function wrapDeg(d: number): number {
  let v = ((d + 180) % 360 + 360) % 360 - 180;
  if (v === -180 && d > 0) v = 180;
  return v;
}

/** The angle from (cx, cy) to (x, y) in the room, degrees clockwise seen from above (0 is to the right). */
export function angleOf(cx: number, cy: number, x: number, y: number): number {
  return (Math.atan2(-(y - cy), x - cx) * 180) / Math.PI;
}

/**
 * Snapping in the room: a spot pulled onto a speaker, the center lines or
 * a quarter-room grid when within `tol` (room units).
 */
export function snapSpot(x: number, y: number, speakers: readonly { x: number; y: number }[], tol = 0.06): [number, number] {
  for (const s of speakers) if (Math.hypot(s.x - x, s.y - y) <= tol * 1.5) return [s.x, s.y];
  const grid = (v: number) => {
    const g = Math.round(v * 4) / 4;
    return Math.abs(g - v) <= tol ? g : v;
  };
  return [grid(x), grid(y)];
}

/** Snapping a rotation to 15 degree steps when within 4 degrees. */
export function snapAngle(d: number): number {
  const g = Math.round(d / 15) * 15;
  return Math.abs(g - d) <= 4 ? wrapDeg(g) : d;
}
