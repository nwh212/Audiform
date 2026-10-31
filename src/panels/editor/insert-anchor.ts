// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Where a track or bus added from a right-click goes: above or below the
 * track that was right-clicked, by which half of it the click was in.
 * Set when the menu opens, used once by the add commands, and forgotten
 * after a short while so a later add from the main menu is not steered.
 */

export interface InsertAnchor {
  trackId: string;
  below: boolean;
}

const LIFETIME_MS = 30_000;
let anchor: (InsertAnchor & { at: number }) | null = null;

/** Remembers the right-clicked track and half (`y` within a row from `top` to `bottom`). */
export function setAnchor(trackId: string, y: number, top: number, bottom: number): void {
  anchor = { trackId, below: y > (top + bottom) / 2, at: Date.now() };
}

/** The anchor, once: it is cleared as it is read. Null when none or too old. */
export function takeAnchor(): InsertAnchor | null {
  const a = anchor;
  anchor = null;
  return a && Date.now() - a.at < LIFETIME_MS ? { trackId: a.trackId, below: a.below } : null;
}

/** Clears the anchor (a menu closed without adding). */
export function clearAnchor(): void {
  anchor = null;
}
