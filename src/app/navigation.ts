// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Where things are in the Multitrack session: the playhead, the end of
 * the content, edges to jump between, and zooming the view.
 */

import { clipLength, sessionEnd } from "../stores/session";
import { editor } from "../stores/editor";
import { session, transport } from "../stores/open-session";
import { maxSppFor, zoomAround } from "../panels/editor/timeline";

/** The Multitrack playhead: where playback is, while playing. */
export const playheadNow = () => (transport.current.playing ? transport.current.position : (session.current?.view.playhead ?? 0));

/** The Multitrack session's view (zoom, scroll, playhead, selection). */
export const sessionView = () => session.current?.view;
export const zoomBy = (factor: number) => {
  const v = sessionView();
  if (!v) return;
  const w = editor.current().width;
  const z = zoomAround({ spp: v.samples_per_pixel, scroll: v.scroll }, factor, w / 2, maxSppFor(w, contentEnd(), session.current?.sample_rate ?? 48000));
  session.setView({ ...v, samples_per_pixel: z.spp, scroll: z.scroll });
};
/** Moves the playhead to the previous or next marker, or (not `markersOnly`) clip edge or selection edge. */
export const jumpEdge = (dir: -1 | 1, markersOnly: boolean) => {
  const doc = session.current;
  if (!doc) return Promise.resolve();
  const here = playheadNow();
  const edges = new Set<number>(doc.markers.map((m) => m.position));
  if (!markersOnly) {
    const media = new Map(doc.media.map((m) => [m.id, m]));
    for (const t of doc.tracks)
      for (const c of t.clips) {
        edges.add(c.start);
        edges.add(c.start + clipLength(c, media.get(c.media_id), doc.sample_rate));
      }
    const sel = doc.view.selection;
    if (sel) {
      edges.add(sel.start);
      edges.add(sel.end);
    }
  }
  const slack = Math.max(1, Math.round(doc.sample_rate / 1000));
  const list = [...edges].filter((e) => (dir < 0 ? e < here - slack : e > here + slack));
  const to = list.length ? (dir < 0 ? Math.max(...list) : Math.min(...list)) : dir < 0 ? 0 : contentEnd();
  return transport.seek(to);
};
/** Where the session's last clip (audio or MIDI) ends. */
export const contentEnd = () => {
  const doc = session.current;
  return doc ? sessionEnd(doc) : 0;
};
/** Where a "clips to ..." selection starts or ends: the chosen clips, or the playhead when none are chosen. */
export const clipSpan = () => {
  const doc = session.current;
  if (!doc) return null;
  const media = new Map(doc.media.map((m) => [m.id, m]));
  let first = Infinity;
  let last = -Infinity;
  let any = false;
  for (const t of doc.tracks)
    for (const c of t.clips) {
      if (!editor.current().selectedClips.has(c.id)) continue;
      any = true;
      first = Math.min(first, c.start);
      last = Math.max(last, c.start + clipLength(c, media.get(c.media_id), doc.sample_rate));
    }
  return any ? { first, last } : { first: playheadNow(), last: playheadNow() };
};
