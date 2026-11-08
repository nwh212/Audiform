// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform.

/**
 * Save Space: which of a session's files to shrink, and the changes that
 * point the session at the smaller copies. The copies themselves are made
 * and checked in the core (`media_save_space`).
 */

import type { MediaRefDoc, Op, SessionDoc } from "../stores/session";
import { folderOf, insideAny } from "./session-media";

export interface SpaceItem {
  path: string;
  /** False when a mono file would change how the session sounds. */
  monoOk: boolean;
}

/** What the core did with one file. */
export interface SpaceOutcome {
  from: string;
  to: string | null;
  bytesBefore: number;
  bytesAfter: number;
  flac: boolean;
  mono: boolean;
  channels: number;
  skipped: string | null;
}

export interface SpaceOptions {
  flac: boolean;
  to24: boolean;
  mono: boolean;
  near: boolean;
}

const key = (p: string, ignoreCase: boolean) => {
  const s = p.replace(/\\/g, "/");
  return ignoreCase ? s.toLowerCase() : s;
};

/**
 * The files the session's clips use that sit inside its folder, once each.
 * A file stays stereo when the session is in 3D Space (where a stereo
 * source has a width of its own) or a clip uses one side of it (Source
 * Channels, or a flip of one side only). `skip` leaves files out, such as
 * one open in the Waveform editor with unsaved edits.
 */
export function spaceItems(doc: SessionDoc, sessionFile: string, ignoreCase: boolean, skip: readonly string[] = []): SpaceItem[] {
  const dir = folderOf(sessionFile);
  const skipped = new Set(skip.map((p) => key(p, ignoreCase)));
  const sided = new Set<string>();
  const used = new Set<string>();
  for (const t of doc.tracks) {
    for (const c of t.clips) {
      used.add(c.media_id);
      const oneSide = c.inverted && (c.invert_channel === "left" || c.invert_channel === "right");
      if (c.channel_mix || oneSide) sided.add(c.media_id);
    }
  }
  const spatial = !!doc.space;
  const out = new Map<string, SpaceItem>();
  for (const m of doc.media) {
    if (!used.has(m.id) || !insideAny(m.path, [dir], ignoreCase) || skipped.has(key(m.path, ignoreCase))) continue;
    const k = key(m.path, ignoreCase);
    const monoOk = !spatial && !sided.has(m.id) && (out.get(k)?.monoOk ?? true);
    out.set(k, { path: m.path, monoOk });
  }
  return [...out.values()];
}

/** The session changes for the copies made: each file's new path, and its new channel count when it became mono. */
export function spaceOps(media: readonly MediaRefDoc[], outcomes: readonly SpaceOutcome[], ignoreCase: boolean): Op[] {
  const made = new Map(outcomes.filter((o) => o.to).map((o) => [key(o.from, ignoreCase), o]));
  const ops: Op[] = [];
  for (const m of media) {
    const o = made.get(key(m.path, ignoreCase));
    if (!o?.to) continue;
    ops.push({ op: "set_media", media: { ...m, path: o.to, relative_path: null, channels: o.channels || m.channels } });
  }
  return ops;
}

/** Bytes as "1.2 GB" and the like. */
export function bytesText(n: number): string {
  const units = ["bytes", "KB", "MB", "GB", "TB"];
  let v = n;
  let u = 0;
  while (v >= 1000 && u < units.length - 1) {
    v /= 1000;
    u++;
  }
  return u === 0 ? `${n} bytes` : `${v.toFixed(v >= 100 ? 0 : 1)} ${units[u]}`;
}
