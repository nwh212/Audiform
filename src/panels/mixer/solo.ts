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

/**
 * The tracks not heard right now: muted ones, and while anything is
 * soloed, every track that is not soloed and does not feed a soloed bus
 * (through its output or a send, directly or through other buses).
 * Their clips are drawn greyed out.
 */
export function silentTracks(doc: SessionDoc | null): Set<string> {
  const out = new Set<string>();
  if (!doc) return out;
  const buses = doc.buses ?? [];
  const anySolo = doc.tracks.some((t) => t.solo) || buses.some((b) => b.solo);
  const byId = new Map(buses.map((b) => [b.id, b]));
  /** True when something sent to `busId` reaches a soloed bus. */
  const reaches = (busId: string, seen: Set<string>): boolean => {
    if (seen.has(busId)) return false;
    seen.add(busId);
    const b = byId.get(busId);
    if (!b) return false;
    if (b.solo) return true;
    const next = [...(b.output?.type === "bus" ? [b.output.bus_id] : []), ...(b.sends ?? []).map((x) => x.bus_id)];
    return next.some((id) => reaches(id, seen));
  };
  for (const t of doc.tracks) {
    if (t.muted) {
      out.add(t.id);
      continue;
    }
    if (!anySolo || t.solo) continue;
    const next = [...(t.output?.type === "bus" ? [t.output.bus_id] : []), ...(t.sends ?? []).map((x) => x.bus_id)];
    if (!next.some((id) => reaches(id, new Set()))) out.add(t.id);
  }
  return out;
}
