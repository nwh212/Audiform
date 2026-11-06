// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Bring Clip to Front and Send Clip to Back: where overlapping clips on a track stack. A track keeps
 * its clips in stacking order (first is the bottom), and a clip change with `layer` moves one clip
 * to that place, so this needs no new data in the session.
 */

import type { ClipChange, SessionDoc } from "../../stores/session";
import { TOP_LAYER } from "./clip-edit";

export type StackMove = "front" | "back";

/**
 * The changes that put the selected clips at the front or the back of their tracks, keeping their order
 * among themselves. Locked clips stay, and so does a clip already there. Empty when nothing would move.
 */
export function stackChanges(doc: SessionDoc, ids: ReadonlySet<string>, move: StackMove): ClipChange[] {
  const out: ClipChange[] = [];
  for (const t of doc.tracks) {
    const clips = t.clips ?? [];
    const picked = clips.filter((c) => ids.has(c.id) && !c.locked);
    if (picked.length === 0) continue;
    const rest = clips.filter((c) => !picked.includes(c));
    const want = move === "front" ? [...rest, ...picked] : [...picked, ...rest];
    if (want.every((c, i) => c === clips[i])) continue;
    // Back: the last one goes to the bottom first, so the first ends up lowest of the group.
    const order = move === "front" ? picked : [...picked].reverse();
    for (const c of order) out.push({ id: c.id, layer: move === "front" ? TOP_LAYER : 0 });
  }
  return out;
}
