// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Bins (named groups of library items, stored as `collections`), the
 * starter set for a podcast library, and the library's show badge.
 */

import type { Library, LibraryCollection } from "../../app/core";

/** Palette tokens used for bins and show badges. */
export const PALETTE = ["--clip-1", "--clip-2", "--clip-3", "--clip-4", "--clip-5", "--clip-6"] as const;

/** The bins a new show library starts with. */
export const STARTER_BINS: readonly { name: string; color: string | null }[] = [
  { name: "Intro / Outro", color: "--clip-2" },
  { name: "Ads", color: "--clip-4" },
  { name: "Promos", color: "--clip-5" },
  { name: "Clips", color: "--clip-1" },
  { name: "Music Beds", color: "--clip-3" },
  { name: "Sound Effects", color: "--clip-6" },
  { name: "Legal Reads", color: null },
];

/** Starter bins the library does not have yet, matched by name ignoring case. */
export function missingStarterBins(lib: Pick<Library, "collections">): { name: string; color: string | null }[] {
  const have = new Set(lib.collections.map((c) => c.name.trim().toLowerCase()));
  return STARTER_BINS.filter((b) => !have.has(b.name.toLowerCase()));
}

/** Up to two letters for a show badge: "Morning Show" gives "MS", "SHOWCAST" gives "SH". */
export function initials(name: string): string {
  const words = name.trim().split(/[\s_\-]+/).filter(Boolean);
  if (words.length === 0) return "?";
  if (words.length === 1) return words[0]!.slice(0, 2).toUpperCase();
  return (words[0]![0]! + words[1]![0]!).toUpperCase();
}

/** A stable palette color for a library that has none set. */
export function defaultColor(name: string): string {
  let h = 0;
  for (const ch of name) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
  return PALETTE[h % PALETTE.length]!;
}

/** The bins an item is in, in bin order. */
export function binsOf(lib: Pick<Library, "collections">, itemId: string): LibraryCollection[] {
  return lib.collections.filter((c) => c.items.includes(itemId));
}
