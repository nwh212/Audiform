// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Track visibility: which tracks the Editor and Mixer list. Hidden tracks
 * still play and mix; only the lists skip them.
 */

interface ClipSpan {
  start: number;
  /** Length on the timeline, in samples. */
  length: number;
}

export interface VisTrack {
  id: string;
  hidden?: boolean;
  clips: { id: string }[];
}

/** The tracks the lists show. */
export function shownTracks<T extends { hidden?: boolean }>(
  tracks: readonly T[],
): T[] {
  return tracks.filter((t) => !t.hidden);
}

/** Ids of tracks whose hidden flag must change to leave only those `keep` accepts showing. */
export function showOnly<T extends VisTrack>(
  tracks: readonly T[],
  keep: (t: T) => boolean,
): { id: string; hidden: boolean }[] {
  const out: { id: string; hidden: boolean }[] = [];
  for (const t of tracks) {
    const hidden = !keep(t);
    if (hidden !== !!t.hidden) out.push({ id: t.id, hidden });
  }
  return out;
}

/** True when any clip of the track overlaps [start, end). A point (start == end) uses start. */
export function hasClipIn(
  spans: readonly ClipSpan[],
  start: number,
  end: number,
): boolean {
  const hi = Math.max(end, start + 1);
  return spans.some((c) => c.start < hi && c.start + c.length > start);
}
