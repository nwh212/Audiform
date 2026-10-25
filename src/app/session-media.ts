// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

/**
 * Keeping a session's audio in its own folder: which files to copy, and
 * the changes that point the session and the Files panel at the copies.
 * The copying itself is in the core (`media_copy_in`).
 */

import type { MediaRefDoc, Op } from "../stores/session";

/** Comparable form of a folder or file path. */
function norm(p: string, ignoreCase: boolean): string {
  const s = p.replace(/\\/g, "/").replace(/\/+$/, "");
  return ignoreCase ? s.toLowerCase() : s;
}

/** Whether `path` is inside any of `dirs`. */
export function insideAny(path: string, dirs: readonly string[], ignoreCase: boolean): boolean {
  const p = norm(path, ignoreCase);
  return dirs.some((d) => {
    const n = norm(d, ignoreCase);
    return n !== "" && p.startsWith(`${n}/`);
  });
}

/** The folder holding a session file. */
export function folderOf(sessionFile: string): string {
  const i = Math.max(sessionFile.lastIndexOf("/"), sessionFile.lastIndexOf("\\"));
  return i > 0 ? sessionFile.slice(0, i) : "";
}

/**
 * The files to copy into a session saved at `sessionFile`: everything
 * outside its folder, and, when `leave` lists library folders, nothing
 * inside those. Each path once, in order.
 */
export function toCopy(paths: readonly string[], sessionFile: string, leave: readonly string[], ignoreCase: boolean): string[] {
  const home = folderOf(sessionFile);
  const seen = new Set<string>();
  const out: string[] = [];
  for (const p of paths) {
    const k = norm(p, ignoreCase);
    if (seen.has(k)) continue;
    seen.add(k);
    if (insideAny(p, [home], ignoreCase) || insideAny(p, leave, ignoreCase)) continue;
    out.push(p);
  }
  return out;
}

/**
 * After copying: the session ops that point its media at the copies,
 * and the Files panel's changes. `moved` maps an old path to its copy.
 */
export function repointPlan(
  media: readonly MediaRefDoc[],
  pool: readonly { id: string; path: string }[],
  moved: ReadonlyMap<string, string>,
  ignoreCase: boolean,
): { ops: Op[]; pool: { id: string; path: string }[] } {
  const to = new Map([...moved].map(([from, copy]) => [norm(from, ignoreCase), copy]));
  const find = (p: string) => {
    const c = to.get(norm(p, ignoreCase));
    return c && c !== p ? c : null;
  };
  const ops: Op[] = [];
  for (const m of media) {
    const c = find(m.path);
    if (c) ops.push({ op: "set_media", media: { ...m, path: c, relative_path: null } });
  }
  const files: { id: string; path: string }[] = [];
  for (const it of pool) {
    const c = find(it.path);
    if (c) files.push({ id: it.id, path: c });
  }
  return { ops, pool: files };
}
