// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Theme pointers: the scene themes draw the text and time-selection
 * I-beam, the resize double arrows and the grab hands in the same glowing
 * blue glass as their arrow; Phosphor draws them as square green pixels.
 * Many of these pointers are chosen by code as the mouse moves (lane
 * edges, track and panel resizing, dragging a window by its title), so a
 * style sheet cannot catch them all: this watches the element under the
 * pointer, reads the pointer it asks for, and swaps in the theme's own,
 * keeping the standard shape as the fallback.
 */

import glassText from "../assets/themes/cursor-text.svg";
import glassEw from "../assets/themes/cursor-ew.svg";
import glassNs from "../assets/themes/cursor-ns.svg";
import glassNwse from "../assets/themes/cursor-nwse.svg";
import glassNesw from "../assets/themes/cursor-nesw.svg";
import glassGrab from "../assets/themes/cursor-grab.svg";
import glassGrabbing from "../assets/themes/cursor-grabbing.svg";
import pxText from "../assets/themes/phosphor-text.svg";
import pxEw from "../assets/themes/phosphor-ew.svg";
import pxNs from "../assets/themes/phosphor-ns.svg";
import pxNwse from "../assets/themes/phosphor-nwse.svg";
import pxNesw from "../assets/themes/phosphor-nesw.svg";
import pxGrab from "../assets/themes/phosphor-grab.svg";
import pxGrabbing from "../assets/themes/phosphor-grabbing.svg";

/** One theme's pictures for each kind of pointer, with the hot spot (the middle of the art). */
interface PointerSet {
  text: string;
  ew: string;
  ns: string;
  nwse: string;
  nesw: string;
  grab: string;
  grabbing: string;
  /** Hot spot in pixels: one value for x and y on every picture, or x and y per picture. */
  hot: number | Record<Kind, [number, number]>;
}

type Kind = "text" | "ew" | "ns" | "nwse" | "nesw" | "grab" | "grabbing";

const GLASS: PointerSet = {
  text: glassText,
  ew: glassEw,
  ns: glassNs,
  nwse: glassNwse,
  nesw: glassNesw,
  grab: glassGrab,
  grabbing: glassGrabbing,
  hot: 10,
};
const PHOSPHOR: PointerSet = {
  text: pxText,
  ew: pxEw,
  ns: pxNs,
  nwse: pxNwse,
  nesw: pxNesw,
  grab: pxGrab,
  grabbing: pxGrabbing,
  // Pixel art at 2 screen pixels per cell, so each picture has its own middle.
  hot: {
    text: [7, 10],
    ew: [11, 5],
    ns: [5, 11],
    nwse: [9, 9],
    nesw: [9, 9],
    grab: [10, 9],
    grabbing: [10, 7],
  },
};

/** Themes with their own pointers. */
const SETS: Record<string, PointerSet> = {
  meadow: GLASS,
  aqua: GLASS,
  moonlight: GLASS,
  prism: GLASS,
  phosphor: PHOSPHOR,
};

/** Which picture stands in for each standard pointer. */
const KIND: Record<string, Kind> = {
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
};

/** The standard pointer a cursor value asks for: its last, fallback keyword. */
export function keywordOf(cursor: string): string {
  const parts = cursor.split(",");
  return (parts[parts.length - 1] ?? "").trim();
}

/** The cursor value to use instead in `theme`, or null to leave it as it is. */
export function themedFor(cursor: string, theme: string): string | null {
  const set = SETS[theme];
  const kw = keywordOf(cursor);
  const kind = KIND[kw];
  if (!set || !kind) return null;
  const [x, y] =
    typeof set.hot === "number" ? [set.hot, set.hot] : set.hot[kind];
  return `url("${set[kind]}") ${x} ${y}, ${kw}`;
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
    const theme = document.documentElement.dataset.theme ?? "";
    if (!SETS[theme]) {
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
    const swap = themedFor(getComputedStyle(el).cursor, theme);
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
