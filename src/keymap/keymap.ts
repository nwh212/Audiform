// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * The active keymap: shipped defaults plus the user's changes.
 *
 * Only the user's changes are saved (in the app config folder), so new
 * defaults in later versions still reach users who never touched them.
 * An action the user cleared is stored as an empty list.
 */

import { writable, type Readable } from "svelte/store";
import { parseChord } from "./chord";
import defaultFile from "./default.json";

export type Bindings = Record<string, string[]>;

export interface KeymapFile {
  format: "altaudio.keymap";
  schema_version: number;
  bindings: Bindings;
}

export const KEYMAP_FORMAT = "altaudio.keymap";
export const KEYMAP_VERSION = 1;

/** Canonicalizes every chord and drops invalid or duplicate ones. */
export function normalizeBindings(input: Bindings): Bindings {
  const out: Bindings = {};
  for (const [id, chords] of Object.entries(input)) {
    const clean: string[] = [];
    for (const c of chords) {
      const parsed = parseChord(c);
      if (parsed && !clean.includes(parsed)) clean.push(parsed);
    }
    out[id] = clean;
  }
  return out;
}

export const DEFAULT_BINDINGS: Bindings = normalizeBindings((defaultFile as KeymapFile).bindings);

const same = (a: readonly string[], b: readonly string[]) =>
  a.length === b.length && a.every((x, i) => x === b[i]);

export class Keymap {
  private overrides: Bindings = {};
  private byChord = new Map<string, string>();
  private readonly tick = writable(0);
  private n = 0;

  /** Changes whenever any binding changes. */
  readonly changed: Readable<number> = this.tick;

  constructor(private readonly defaults: Bindings = DEFAULT_BINDINGS) {
    this.rebuild();
  }

  /** Replaces the user's changes, for example after loading them from disk. */
  setOverrides(overrides: Bindings): void {
    this.overrides = normalizeBindings(overrides);
    this.rebuild();
  }

  /** The user's changes, ready to save. */
  getOverrides(): Bindings {
    return structuredClone(this.overrides);
  }

  /** Chords bound to an action, in display order. */
  chordsFor(id: string): readonly string[] {
    return this.overrides[id] ?? this.defaults[id] ?? [];
  }

  /** First chord for menus and tooltips. */
  displayFor(id: string): string | undefined {
    return this.chordsFor(id)[0];
  }

  /** Action bound to a chord, if any. */
  actionFor(chord: string): string | undefined {
    return this.byChord.get(chord);
  }

  isChanged(id: string): boolean {
    return id in this.overrides;
  }

  /**
   * Makes `chord` the only shortcut for `id`. If another action held the
   * chord, it loses it. Returns the id of the action that lost it.
   */
  assign(id: string, chordText: string): string | undefined {
    const chord = parseChord(chordText);
    if (!chord) throw new Error(`Not a valid shortcut: ${chordText}`);
    const previous = this.byChord.get(chord);
    if (previous && previous !== id) {
      this.setFor(previous, this.chordsFor(previous).filter((c) => c !== chord));
    }
    this.setFor(id, [chord]);
    this.rebuild();
    return previous && previous !== id ? previous : undefined;
  }

  /** Removes every shortcut from an action. */
  clear(id: string): void {
    this.setFor(id, []);
    this.rebuild();
  }

  /** Puts one action back to its shipped shortcut, taking it from anyone who holds it. */
  reset(id: string): void {
    delete this.overrides[id];
    for (const chord of this.defaults[id] ?? []) {
      const holder = this.byChord.get(chord);
      if (holder && holder !== id) {
        this.setFor(
          holder,
          this.chordsFor(holder).filter((c) => c !== chord),
        );
      }
    }
    this.rebuild();
  }

  resetAll(): void {
    this.overrides = {};
    this.rebuild();
  }

  // Internals

  private setFor(id: string, chords: string[]): void {
    if (same(chords, this.defaults[id] ?? [])) delete this.overrides[id];
    else this.overrides[id] = chords;
  }

  private rebuild(): void {
    this.byChord.clear();
    const ids = new Set([...Object.keys(this.defaults), ...Object.keys(this.overrides)]);
    for (const id of ids) {
      for (const chord of this.chordsFor(id)) {
        // First one wins if a hand-edited file has a duplicate.
        if (!this.byChord.has(chord)) this.byChord.set(chord, id);
      }
    }
    this.tick.set(++this.n);
  }
}
