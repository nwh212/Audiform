// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Playlist logic: an ordered list of range markers that previews back to
 * back and exports as one file per range. The order is saved in the
 * session; markers deleted elsewhere simply drop out of it.
 */

import type { MarkerDoc, SessionDoc } from "../../stores/session";

/** Range markers in playlist order. Stale ids and point markers are skipped. */
export function playlistItems(doc: SessionDoc): MarkerDoc[] {
  const byId = new Map(doc.markers.map((m) => [m.id, m]));
  return (doc.playlist ?? []).flatMap((id) => {
    const m = byId.get(id);
    return m && m.duration > 0 ? [m] : [];
  });
}

/** Range markers not in the playlist yet, in time order. */
export function available(doc: SessionDoc): MarkerDoc[] {
  const inList = new Set(doc.playlist ?? []);
  return doc.markers.filter((m) => m.duration > 0 && !inList.has(m.id)).sort((a, b) => a.position - b.position);
}

/** The playlist with stale ids removed, so edits start from what is shown. */
function clean(doc: SessionDoc): string[] {
  return playlistItems(doc).map((m) => m.id);
}

export function withAdded(doc: SessionDoc, ids: readonly string[]): string[] {
  const cur = clean(doc);
  return [...cur, ...ids.filter((id) => !cur.includes(id))];
}

export function withRemoved(doc: SessionDoc, id: string): string[] {
  return clean(doc).filter((x) => x !== id);
}

export function withMoved(doc: SessionDoc, id: string, by: -1 | 1): string[] {
  const cur = clean(doc);
  const i = cur.indexOf(id);
  const j = i + by;
  if (i < 0 || j < 0 || j >= cur.length) return cur;
  [cur[i], cur[j]] = [cur[j]!, cur[i]!];
  return cur;
}

/**
 * While previewing item `index`: where playback should go once the
 * playhead passes its end. Null means stop.
 */
export function nextStart(items: readonly MarkerDoc[], index: number): { index: number; at: number } | null {
  const n = items[index + 1];
  return n ? { index: index + 1, at: n.position } : null;
}

/** File names for exporting each item: "01 Cold open", safe for any system, unique. */
export function exportNames(items: readonly MarkerDoc[]): string[] {
  const used = new Set<string>();
  const width = String(items.length).length < 2 ? 2 : String(items.length).length;
  return items.map((m, i) => {
    const base = `${String(i + 1).padStart(width, "0")} ${m.name}`.replace(/[<>:"/\\|?*\u0000-\u001f]+/g, " ").replace(/\s+/g, " ").trim();
    let name = base;
    for (let k = 2; used.has(name.toLowerCase()); k++) name = `${base} (${k})`;
    used.add(name.toLowerCase());
    return name;
  });
}
