// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Track header and mixer changes as one op. With exclusive solo on
 * (Settings > Multitrack), soloing a track unsolos the other tracks and
 * buses in the same undoable step.
 */

import type { Op, SessionDoc } from "../../stores/session";
import { settings } from "../../stores/settings";

export function trackOp(doc: SessionDoc | null, id: string, changes: Record<string, unknown>, exclusive = settings.current.soloExclusive): Op {
  const op: Op = { op: "set_track", id, changes } as Op;
  if (!exclusive || changes.solo !== true || !doc) return op;
  const others = doc.tracks.filter((t) => t.id !== id && t.solo);
  const buses = (doc.buses ?? []).filter((b) => b.solo);
  if (others.length === 0 && buses.length === 0) return op;
  return {
    op: "batch",
    ops: [
      op,
      ...others.map((t) => ({ op: "set_track", id: t.id, changes: { solo: false } }) as Op),
      ...buses.map((b) => ({ op: "set_bus", id: b.id, changes: { solo: false } }) as Op),
    ],
  };
}
