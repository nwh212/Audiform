// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * A track name from a file name: the extension goes, and so does the
 * number call recorders put in front of each speaker ("1-name.flac"
 * becomes "name"). Underscores read as spaces.
 */
export function trackNameFromFile(fileName: string): string {
  const base = fileName.replace(/^.*[\\/]/, "");
  const s = base
    .replace(/\.[^.]+$/, "")
    .replace(/^\d{1,3}[-_ ]+(?=\S)/, "")
    .replace(/_+/g, " ")
    .trim();
  return s || base;
}

/**
 * Names for `count` new tracks that do not clash with `existing`. With
 * no base they continue "Track N"; with a base ("Guest") the first is
 * the base itself when free, then "Guest 2", "Guest 3" and so on.
 */
export function newTrackNames(existing: readonly string[], count: number, base = ""): string[] {
  const taken = new Set(existing);
  const out: string[] = [];
  const b = base.trim();
  let n = b ? 1 : existing.length + 1;
  while (out.length < count) {
    const name = b ? (n === 1 ? b : `${b} ${n}`) : `Track ${n}`;
    if (!taken.has(name)) {
      out.push(name);
      taken.add(name);
    }
    n++;
  }
  return out;
}
