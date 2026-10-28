// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Four-corner resizing. Where the handles between panels meet (a cross,
 * or a T), grabbing the spot where they meet drags all of them at once,
 * so a panel edge and its neighbour's move together in both directions.
 * "Meet" is forgiving: handles that end within a few pixels of the grab
 * point count, so corners that are only roughly aligned still work.
 *
 * The docking library starts a drag from `pointerdown` on a handle, then
 * follows the pointer on the document. So starting every handle at the
 * same point is all it takes: this passes the grab on to the others.
 */

export interface Box {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/** How close, in pixels, a handle must be to the grab point to come along. */
export const CORNER_REACH = 10;

/** Handles (by index) whose area, widened by `reach`, holds the point. */
export function handlesAt(x: number, y: number, boxes: readonly Box[], reach = CORNER_REACH): number[] {
  const out: number[] = [];
  boxes.forEach((b, i) => {
    if (x >= b.left - reach && x <= b.right + reach && y >= b.top - reach && y <= b.bottom + reach) out.push(i);
  });
  return out;
}

const SASH = ".dv-sash";

/** Wires the behaviour onto the docking area. Returns the stop function. */
export function enableCornerDrag(host: HTMLElement): () => void {
  const others = (target: HTMLElement, x: number, y: number): HTMLElement[] => {
    const all = [...host.querySelectorAll<HTMLElement>(SASH)].filter((s) => s !== target && !s.classList.contains("dv-disabled"));
    const at = handlesAt(
      x,
      y,
      all.map((s) => s.getBoundingClientRect()),
    );
    return at.map((i) => all[i]!);
  };

  /** How deeply a handle is nested in the layout: 0 is the outermost split. */
  const depth = (s: HTMLElement) => {
    let n = 0;
    for (let el = s.parentElement; el && el !== host; el = el.parentElement) if (el.classList.contains("dv-split-view-container")) n++;
    return n;
  };

  const onDown = (e: PointerEvent) => {
    if (!e.isTrusted || e.button !== 0) return;
    const target = (e.target as HTMLElement | null)?.closest<HTMLElement>(SASH);
    if (!target || target.classList.contains("dv-disabled")) return;
    const together = others(target, e.clientX, e.clientY);
    if (together.length === 0) return;
    // The library re-lays-out inner splits from their saved proportions when
    // an outer one resizes, which would undo an inner handle's move. So take
    // over the grab and start every handle ourselves, outermost first: the
    // inner ones then get the last word on each pointer move.
    e.stopPropagation();
    e.preventDefault();
    for (const s of [target, ...together].sort((a, b) => depth(a) - depth(b))) {
      s.dispatchEvent(
        new PointerEvent("pointerdown", {
          bubbles: false,
          cancelable: true,
          button: 0,
          buttons: 1,
          clientX: e.clientX,
          clientY: e.clientY,
          pointerId: e.pointerId,
          pointerType: e.pointerType,
        }),
      );
    }
  };

  // A hint: over a meeting point the pointer becomes a four-way mover.
  let hinted: HTMLElement | null = null;
  const onMove = (e: PointerEvent) => {
    if (e.buttons !== 0) return;
    const target = (e.target as HTMLElement | null)?.closest<HTMLElement>(SASH);
    const together = !!target && others(target, e.clientX, e.clientY).length > 0;
    const want = together ? target : null;
    if (hinted && hinted !== want) hinted.style.removeProperty("cursor");
    if (want) want.style.setProperty("cursor", "move", "important");
    hinted = want;
  };

  host.addEventListener("pointerdown", onDown, true);
  host.addEventListener("pointermove", onMove, true);
  return () => {
    host.removeEventListener("pointerdown", onDown, true);
    host.removeEventListener("pointermove", onMove, true);
    hinted?.style.removeProperty("cursor");
  };
}
