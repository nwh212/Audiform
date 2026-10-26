// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Selection rules behind Edit > Select (and the empty-track right-click
 * menu). Pure functions over clip start positions, so they are easy to test.
 */

export interface Positioned {
  id: string;
  start: number;
}

/**
 * Where a "clips to ..." selection counts from: the earliest chosen clip,
 * or the playhead when no clip is chosen.
 */
export function anchorStart(all: readonly Positioned[], chosen: ReadonlySet<string>, playhead: number): number {
  let first = Infinity;
  for (const c of all) if (chosen.has(c.id)) first = Math.min(first, c.start);
  return Number.isFinite(first) ? first : playhead;
}

/** Ids of clips that start at or after `from`. */
export function clipsFrom(clips: readonly Positioned[], from: number): string[] {
  return clips.filter((c) => c.start >= from).map((c) => c.id);
}

/** Ids of clips that start at or before `to`. */
export function clipsUpTo(clips: readonly Positioned[], to: number): string[] {
  return clips.filter((c) => c.start <= to).map((c) => c.id);
}

/**
 * The next clip in a track: after the last chosen clip, or the first clip
 * at or after the playhead when none in this track is chosen.
 */
export function nextClipId(track: readonly Positioned[], chosen: ReadonlySet<string>, playhead: number): string | null {
  const sorted = [...track].sort((a, b) => a.start - b.start);
  const last = sorted.filter((c) => chosen.has(c.id)).at(-1);
  const next = last ? sorted.find((c) => c.start > last.start) : sorted.find((c) => c.start >= playhead);
  return next?.id ?? null;
}
