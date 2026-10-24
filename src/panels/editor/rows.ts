// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

/**
 * The Editor's rows: audio tracks with bus tracks placed among them. A
 * bus stores how many tracks sit above it (`position`), so moving a track
 * never drags a bus along with it, and a bus can sit anywhere.
 */

import type { BusDoc, Op, SessionDoc, TrackDoc } from "../../stores/session";
import { defaultTrackColor } from "./timeline";

export type Row =
  | { kind: "track"; id: string; track: TrackDoc; /** Index among the listed tracks. */ i: number }
  | { kind: "bus"; id: string; bus: BusDoc };

/** Where a bus sits: below this many tracks of the full list. */
export function busSlot(b: BusDoc, tracks: number): number {
  const p = b.position ?? tracks;
  return Math.max(0, Math.min(tracks, p));
}

/** Every row in order, the hidden tracks included (marked by `listed`). */
function allRows(doc: SessionDoc, listed: ReadonlySet<string>): (Row & { listed: boolean })[] {
  const tracks = doc.tracks;
  const buses = doc.buses ?? [];
  const out: (Row & { listed: boolean })[] = [];
  let i = 0;
  for (let k = 0; k <= tracks.length; k++) {
    for (const b of buses) if (busSlot(b, tracks.length) === k) out.push({ kind: "bus", id: b.id, bus: b, listed: true });
    const t = tracks[k];
    if (!t) continue;
    const shown = listed.has(t.id);
    out.push({ kind: "track", id: t.id, track: t, i: shown ? i : -1, listed: shown });
    if (shown) i++;
  }
  return out;
}

/** The rows the Editor shows, in order. `listed` holds the ids of the tracks it lists. */
export function laneRows(doc: SessionDoc | null, listed: ReadonlySet<string>): Row[] {
  if (!doc) return [];
  return allRows(doc, listed).filter((r) => r.listed);
}

/**
 * The edits that move row `id` to just above the shown row `beforeId`
 * (or to the bottom when null), as one undoable step: the track or bus
 * order, each bus's place among the tracks, and, when tracks move, the
 * colors of tracks still on the default color (which follows the row),
 * so every track keeps the color it had. Empty when nothing moves.
 */
export function moveRowOps(doc: SessionDoc, listed: ReadonlySet<string>, id: string, beforeId: string | null): Op[] {
  const rows = allRows(doc, listed);
  const from = rows.findIndex((r) => r.id === id);
  if (from < 0) return [];
  const moving = rows[from]!;
  const rest = rows.filter((r) => r.id !== id);
  let at = beforeId === null ? rest.length : rest.findIndex((r) => r.id === beforeId);
  if (at < 0) at = rest.length;
  const next = [...rest.slice(0, at), moving, ...rest.slice(at)];
  if (next.every((r, k) => r.id === rows[k]!.id)) return [];

  const ops: Op[] = [];
  if (moving.kind === "track") {
    // Keep every track's color: default colors follow the listed row.
    for (const r of rows) {
      if (r.kind === "track" && r.listed && !r.track.color) {
        ops.push({ op: "set_track", id: r.id, changes: { color: defaultTrackColor(r.i) } });
      }
    }
    const to = next.filter((r) => r.kind === "track").findIndex((r) => r.id === id);
    ops.push({ op: "move_track", id, to });
  } else {
    const to = next.filter((r) => r.kind === "bus").findIndex((r) => r.id === id);
    const was = (doc.buses ?? []).findIndex((b) => b.id === id);
    if (to !== was) ops.push({ op: "move_bus", id, to });
  }
  // Each bus's place: the tracks above it in the new order.
  const total = doc.tracks.length;
  let above = 0;
  for (const r of next) {
    if (r.kind === "track") {
      above++;
      continue;
    }
    // Below the last track is stored as no position at all.
    const stored = r.bus.position == null || r.bus.position >= total ? null : r.bus.position;
    const want = above >= total ? null : above;
    if (stored !== want) ops.push({ op: "set_bus", id: r.id, changes: want === null ? { clear_position: true } : { position: want } });
  }
  return ops;
}
