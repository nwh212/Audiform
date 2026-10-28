// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Theme pointers: a theme file can draw the text and time-selection
 * I-beam, the resize double arrows, the grab hands and the busy pointer
 * (its `pointers` entry, set here by the theme loader when it applies the
 * theme). Many of these pointers are chosen by code as the mouse moves (lane
 * edges, track and panel resizing, dragging a window by its title), so a
 * style sheet cannot catch them all: this watches the element under the
 * pointer, reads the pointer it asks for, and swaps in the theme's own,
 * keeping the standard shape as the fallback.
 */

import type { PointerKind } from "../themes/validate";

/** One picture for a kind of pointer, and its hot spot in pixels. */
export interface ResolvedPointer {
  url: string;
  hot: [number, number];
}

/** The pointer pictures of the theme in use, by kind. Themes without pointers have none. */
export type ResolvedPointers = Partial<Record<PointerKind, ResolvedPointer>>;

let active: ResolvedPointers | null = null;

/** Sets the pointers of the theme in use (null for none). The loader calls this when it applies a theme. */
export function setThemePointers(pointers: ResolvedPointers | null): void {
  active = pointers;
}

/** Which picture stands in for each standard pointer. */
const KIND: Record<string, PointerKind> = {
  text: "text",
  "vertical-text": "text",
  "ew-resize": "ew",
  "col-resize": "ew",
  "e-resize": "ew",
  "w-resize": "ew",
  "ns-resize": "ns",
  "row-resize": "ns",
  "n-resize": "ns",
  "s-resize": "ns",
  "nwse-resize": "nwse",
  "nw-resize": "nwse",
  "se-resize": "nwse",
  "nesw-resize": "nesw",
  "ne-resize": "nesw",
  "sw-resize": "nesw",
  grab: "grab",
  grabbing: "grabbing",
  move: "move",
  wait: "busy",
  progress: "busy",
};

/** The standard pointer a cursor value asks for: its last, fallback keyword. */
export function keywordOf(cursor: string): string {
  const parts = cursor.split(",");
  return (parts[parts.length - 1] ?? "").trim();
}

/** The cursor value to use instead with `pointers`, or null to leave it as it is. */
export function themedFor(cursor: string, pointers: ResolvedPointers | null = active): string | null {
  const kw = keywordOf(cursor);
  const kind = KIND[kw];
  const p = kind ? pointers?.[kind] : undefined;
  if (!p) return null;
  return `url("${p.url}") ${p.hot[0]} ${p.hot[1]}, ${kw}`;
}

/** Starts watching. Returns a function that stops and puts everything back. */
export function watchThemeCursor(): () => void {
  let pending: Element | null = null;
  let frame = 0;
  /** What each element had inline before a swap (code may set pointers inline). */
  const original = new WeakMap<
    HTMLElement,
    { value: string; priority: string }
  >();
  /** The swap last put on each element, to tell it from a change made since by code. */
  const ours = new WeakMap<HTMLElement, string>();
  const touched = new Set<HTMLElement>();

  const putBack = (el: HTMLElement) => {
    const o = original.get(el);
    if (o?.value) el.style.setProperty("cursor", o.value, o.priority);
    else el.style.removeProperty("cursor");
  };
  const restore = (el: HTMLElement) => {
    if (ours.get(el) === el.style.getPropertyValue("cursor")) putBack(el);
    ours.delete(el);
    touched.delete(el);
  };

  const update = () => {
    frame = 0;
    const el = pending;
    pending = null;
    if (!(el instanceof HTMLElement)) return;
    if (!active) {
      for (const t of [...touched]) restore(t);
      return;
    }
    // What the element asks for, without our own swap in the way. If code
    // changed its pointer since the swap, that change is the new original.
    const inline = el.style.getPropertyValue("cursor");
    if (ours.get(el) !== inline)
      original.set(el, {
        value: inline,
        priority: el.style.getPropertyPriority("cursor"),
      });
    else putBack(el);
    const swap = themedFor(getComputedStyle(el).cursor);
    if (swap) {
      el.style.setProperty("cursor", swap, "important");
      ours.set(el, el.style.getPropertyValue("cursor"));
      touched.add(el);
    } else {
      ours.delete(el);
      touched.delete(el);
    }
  };

  const onMove = (e: PointerEvent) => {
    pending = e.target as Element;
    if (!frame) frame = requestAnimationFrame(update);
  };
  window.addEventListener("pointermove", onMove, true);
  window.addEventListener("pointerdown", onMove, true);
  return () => {
    window.removeEventListener("pointermove", onMove, true);
    window.removeEventListener("pointerdown", onMove, true);
    if (frame) cancelAnimationFrame(frame);
    for (const t of [...touched]) restore(t);
  };
}
