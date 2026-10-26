// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Glass pointers for the scene themes: the text and time-selection
 * I-beam and the resize double arrows drawn in the same glowing blue
 * glass as the themes' arrow. Many of these pointers are chosen by code
 * as the mouse moves (lane edges, track and panel resizing), so a style
 * sheet cannot catch them all: this watches the element under the
 * pointer, reads the pointer it asks for, and swaps in the glass one,
 * keeping the standard shape as the fallback.
 */

import textUrl from "../assets/themes/cursor-text.svg";
import ewUrl from "../assets/themes/cursor-ew.svg";
import nsUrl from "../assets/themes/cursor-ns.svg";
import nwseUrl from "../assets/themes/cursor-nwse.svg";
import neswUrl from "../assets/themes/cursor-nesw.svg";

/** Themes that use the glass pointers. */
const GLASS_THEMES = new Set(["meadow", "aqua", "moonlight", "prism"]);

/** Each standard pointer, by the glass one that replaces it (its picture and its hot spot, 21 px art). */
const GLASS: Record<string, string> = {
  text: `url("${textUrl}") 10 10`,
  "vertical-text": `url("${textUrl}") 10 10`,
  "ew-resize": `url("${ewUrl}") 10 10`,
  "col-resize": `url("${ewUrl}") 10 10`,
  "e-resize": `url("${ewUrl}") 10 10`,
  "w-resize": `url("${ewUrl}") 10 10`,
  "ns-resize": `url("${nsUrl}") 10 10`,
  "row-resize": `url("${nsUrl}") 10 10`,
  "n-resize": `url("${nsUrl}") 10 10`,
  "s-resize": `url("${nsUrl}") 10 10`,
  "nwse-resize": `url("${nwseUrl}") 10 10`,
  "nw-resize": `url("${nwseUrl}") 10 10`,
  "se-resize": `url("${nwseUrl}") 10 10`,
  "nesw-resize": `url("${neswUrl}") 10 10`,
  "ne-resize": `url("${neswUrl}") 10 10`,
  "sw-resize": `url("${neswUrl}") 10 10`,
};

/** The standard pointer a cursor value asks for: its last, fallback keyword. */
export function keywordOf(cursor: string): string {
  const parts = cursor.split(",");
  return (parts[parts.length - 1] ?? "").trim();
}

/** The cursor value to use instead, or null to leave it as it is. */
export function glassFor(cursor: string): string | null {
  const kw = keywordOf(cursor);
  const glass = GLASS[kw];
  return glass ? `${glass}, ${kw}` : null;
}

/** Starts watching. Returns a function that stops and puts everything back. */
export function watchGlassCursor(): () => void {
  let pending: Element | null = null;
  let frame = 0;
  /** What each element had inline before a swap (code may set pointers inline). */
  const original = new WeakMap<HTMLElement, { value: string; priority: string }>();
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
    if (!GLASS_THEMES.has(document.documentElement.dataset.theme ?? "")) {
      for (const t of [...touched]) restore(t);
      return;
    }
    // What the element asks for, without our own swap in the way. If code
    // changed its pointer since the swap, that change is the new original.
    const inline = el.style.getPropertyValue("cursor");
    if (ours.get(el) !== inline) original.set(el, { value: inline, priority: el.style.getPropertyPriority("cursor") });
    else putBack(el);
    const swap = glassFor(getComputedStyle(el).cursor);
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
