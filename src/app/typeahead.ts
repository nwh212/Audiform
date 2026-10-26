// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Type to jump in a list or menu: typing letters moves to the first row
 * whose label starts with them. Letters typed in quick succession build
 * a longer prefix ("re" goes past "Razor" to "Reverb"); typing the same
 * letter again moves on to the next row starting with it.
 */

/** How long a pause starts a new prefix, in milliseconds. */
const PAUSE_MS = 800;

export interface Typeahead {
  /**
   * The row to jump to after `key` is typed, or null when the key is not
   * a letter to jump by or nothing matches. `labels` is every row's label
   * (null for rows that cannot be picked); `current` is the highlighted row.
   */
  next(key: string, labels: readonly (string | null)[], current: number, now?: number): number | null;
}

/** True for a key that types a letter, digit or symbol (not Space, not a named key). */
export function isTypingKey(e: { key: string; ctrlKey: boolean; altKey: boolean; metaKey: boolean }): boolean {
  return e.key.length === 1 && e.key !== " " && !e.ctrlKey && !e.altKey && !e.metaKey;
}

export function typeahead(): Typeahead {
  let prefix = "";
  let last = 0;
  return {
    next(key, labels, current, now = Date.now()) {
      if (key.length !== 1 || key === " ") return null;
      const k = key.toLowerCase();
      prefix = now - last > PAUSE_MS ? k : prefix + k;
      last = now;
      // The same letter again: step through the rows starting with it.
      const repeat = prefix.length > 1 && [...prefix].every((c) => c === k);
      const want = repeat ? k : prefix;
      const n = labels.length;
      // A longer prefix may still match the row already highlighted.
      const from = want.length > 1 && current >= 0 ? current : current + 1;
      for (let i = 0; i < n; i++) {
        const at = (((from + i) % n) + n) % n;
        const l = labels[at];
        if (l !== null && l !== undefined && l.trim().toLowerCase().startsWith(want)) return at;
      }
      return null;
    },
  };
}
