// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/** Where files dragged over the Multitrack lanes would land, shown before the drop. */

import { xOf, type Viewport } from "./timeline";

export interface GhostItem {
  name: string;
  x: number;
  w: number;
}

export interface DropGhost {
  top: number;
  height: number;
  /** The files would go on a new track below the last. */
  newTrack: boolean;
  items: GhostItem[];
}

/**
 * The boxes for the dropped files, from session sample `start`: one after
 * another, or with `manyAsTracks` (and more than one file) only the first,
 * named with how many more start at the same time on the tracks below.
 * Files whose length is unknown (null) are left out.
 */
export function ghostItems(infos: readonly ({ name: string; len: number } | null)[], start: number, view: Viewport, manyAsTracks: boolean): GhostItem[] {
  let pos = start;
  const items: GhostItem[] = [];
  const side = manyAsTracks && infos.filter(Boolean).length > 1;
  for (const i of infos) {
    if (!i) continue;
    items.push({ name: i.name, x: xOf(pos, view), w: Math.max(4, i.len / view.spp) });
    // Side by side, the ghost shows the first file; the rest start at the same time below.
    if (side) break;
    pos += i.len;
  }
  const more = side ? infos.filter(Boolean).length - 1 : 0;
  if (more > 0) items[0]!.name = `${items[0]!.name} (+${more} on the tracks below)`;
  return items;
}
