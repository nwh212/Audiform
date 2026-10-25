// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

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
  return { x: c(p?.x, -1, 1, d.x), y: c(p?.y, -1, 1, d.y), z: c(p?.z, 0, 1, d.z), size: c(p?.size, 0, 1, d.size), width: c(p?.width, 0, 1, d.width) };
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

/** Where a stereo source's two sides sit, left to right. */
export function sides(p: SpacePosDoc): [number, number] {
  return [Math.max(-1, p.x - p.width), Math.min(1, p.x + p.width)];
}
