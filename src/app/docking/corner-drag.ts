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

/** A made-up pointer event at (x, y), copying the real one's pointer. */
function pointer(type: string, like: PointerEvent, x: number, y: number): PointerEvent {
  return new PointerEvent(type, {
    bubbles: type !== "pointerdown",
    cancelable: true,
    button: 0,
    buttons: type === "pointerup" ? 0 : 1,
    clientX: x,
    clientY: y,
    pointerId: like.pointerId,
    pointerType: like.pointerType,
  });
}

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
    const all = [target, ...together].sort((a, b) => depth(a) - depth(b));
    for (const s of all) s.dispatchEvent(pointer("pointerdown", e, e.clientX, e.clientY));
    // On release the library ends the outer handle first, which lays the
    // inner splits out again from their old proportions before they save
    // their own. So once every handle has let go, drag each inner handle
    // again on its own, from where it now is to where it was let go.
    const outer = depth(all[0]!);
    const inner = all.filter((s) => depth(s) > outer);
    if (inner.length === 0) return;
    const onUp = (u: PointerEvent) => {
      if (!u.isTrusted) return;
      window.removeEventListener("pointerup", onUp, true);
      requestAnimationFrame(() => {
        for (const s of inner) {
          if (!s.isConnected) continue;
          const r = s.getBoundingClientRect();
          const cx = (r.left + r.right) / 2;
          const cy = (r.top + r.bottom) / 2;
          s.dispatchEvent(pointer("pointerdown", u, cx, cy));
          const doc = s.ownerDocument;
          doc.dispatchEvent(pointer("pointermove", u, u.clientX, u.clientY));
          doc.dispatchEvent(pointer("pointerup", u, u.clientX, u.clientY));
        }
      });
    };
    window.addEventListener("pointerup", onUp, true);
  };

  // A hint: over a meeting point the pointer becomes a four-way mover. This is
  // a class on the docking area (a style rule gives the handles the move
  // pointer), not a pointer set on a handle, so the theme pointers and the
  // handles' own resize pointers never get tangled.
  let hot = false;
  const setHot = (on: boolean) => {
    if (on === hot) return;
    hot = on;
    host.classList.toggle("corner-hot", on);
  };
  const onMove = (e: PointerEvent) => {
    if (e.buttons !== 0) return;
    const target = (e.target as HTMLElement | null)?.closest<HTMLElement>(SASH);
    setHot(!!target && others(target, e.clientX, e.clientY).length > 0);
  };

  host.addEventListener("pointerdown", onDown, true);
  host.addEventListener("pointermove", onMove, true);
  return () => {
    host.removeEventListener("pointerdown", onDown, true);
    host.removeEventListener("pointermove", onMove, true);
    setHot(false);
  };
}
