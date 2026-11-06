// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/** Marker kinds, their names and colors, and making new markers. */

import type { MarkerDoc, MarkerKind } from "../../stores/session";

export const KIND_LABELS: Record<MarkerKind, string> = {
  cue: "Cue",
  subclip: "Subclip",
  chapter: "Chapter",
  ad_slot: "Ad Slot",
  social_clip: "Social Clip",
  cd_track: "CD Track",
  cart_timer: "Cart Timer",
};

/** Design token for each kind's color. */
export const KIND_COLORS: Record<MarkerKind, string> = {
  cue: "--clip-5",
  subclip: "--clip-1",
  chapter: "--clip-3",
  ad_slot: "--clip-4",
  social_clip: "--clip-6",
  cd_track: "--clip-2",
  cart_timer: "--clip-7",
};

/** A marker's color for CSS: its own, or its type's. */
export function markerCss(m: Pick<MarkerDoc, "kind" | "color">): string {
  return m.color ? m.color : `var(${KIND_COLORS[m.kind]})`;
}

export function newMarker(
  id: string,
  name: string,
  position: number,
  duration = 0,
  kind: MarkerKind = "cue",
): MarkerDoc {
  return {
    id,
    name,
    kind,
    position: Math.max(0, Math.round(position)),
    duration: Math.max(0, Math.round(duration)),
    description: "",
    chapter: null,
    contracted_duration: null,
  };
}

/** "Marker 3" style name that is not taken yet. */
export function nextMarkerName(
  markers: readonly MarkerDoc[],
  base = "Marker",
): string {
  const used = new Set(markers.map((m) => m.name));
  for (let n = markers.length + 1; ; n++) {
    const name = `${base} ${n}`;
    if (!used.has(name)) return name;
  }
}

/** The marker whose flag is under `x` (pixels), nearest first, within `tolerance`. */
export function markerAt(
  markers: readonly MarkerDoc[],
  x: number,
  xOf: (sample: number) => number,
  tolerance = 5,
): MarkerDoc | null {
  let best: MarkerDoc | null = null;
  let bestD = Infinity;
  for (const m of markers) {
    const d = Math.abs(xOf(m.position) - x);
    if (d <= tolerance && d < bestD) {
      best = m;
      bestD = d;
    }
  }
  return best;
}

/** The markers whose ids are in `ids`, in time order (markers at the same spot keep their list order). */
export function pickedMarkers(markers: readonly MarkerDoc[], ids: ReadonlySet<string>): MarkerDoc[] {
  return markers.filter((m) => ids.has(m.id)).sort((a, b) => a.position - b.position);
}

/** Ids of every marker from `a` to `b` in time order, both ends included (Shift+click). Empty when either is missing. */
export function markersBetween(markers: readonly MarkerDoc[], a: string, b: string): string[] {
  const ordered = [...markers].sort((x, y) => x.position - y.position);
  const i = ordered.findIndex((m) => m.id === a);
  const j = ordered.findIndex((m) => m.id === b);
  if (i < 0 || j < 0) return [];
  return ordered.slice(Math.min(i, j), Math.max(i, j) + 1).map((m) => m.id);
}

/** The undo label for deleting `n` markers. */
export function deleteLabel(n: number): string {
  return n === 1 ? "Delete Marker" : `Delete ${n} Markers`;
}
