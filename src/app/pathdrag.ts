// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Dragging files between panels inside the app, such as Drive Manager
 * rows onto the Files panel. Pointer based, like panel docking, because
 * HTML drag and drop is turned off to make OS file drops work.
 */

import type { FileDropRouter } from "./filedrop";

/** Pixels the pointer must move before a press becomes a drag. */
export const DRAG_THRESHOLD = 5;

/**
 * Call from a pointerdown handler. The drag starts only once the
 * pointer moves past the threshold; a plain click is left alone.
 * `paths` is read when the drag starts, so selection changes made by
 * the same click are included.
 */
export function beginPathDrag(
  down: PointerEvent,
  router: FileDropRouter,
  paths: () => string[],
  label: (count: number) => string = (n) => (n === 1 ? "1 file" : `${n} files`),
  /** The panel the drag starts in; dropping back on it does nothing. */
  from?: HTMLElement,
): void {
  if (down.button !== 0) return;
  // Stops the browser from starting a text selection that would follow the drag.
  down.preventDefault();
  const startX = down.clientX;
  const startY = down.clientY;
  let dragging: string[] | null = null;
  let ghost: HTMLDivElement | null = null;

  const move = (e: PointerEvent) => {
    if (!dragging) {
      if (Math.hypot(e.clientX - startX, e.clientY - startY) < DRAG_THRESHOLD) return;
      dragging = paths();
      if (dragging.length === 0) return cleanup();
      ghost = document.createElement("div");
      ghost.className = "path-drag-ghost";
      ghost.textContent = label(dragging.length);
      document.body.appendChild(ghost);
      document.body.classList.add("path-dragging");
      window.getSelection()?.removeAllRanges();
    }
    if (ghost) {
      ghost.style.left = `${e.clientX + 12}px`;
      ghost.style.top = `${e.clientY + 8}px`;
    }
    router.dragOver(e.clientX, e.clientY, from, dragging ?? []);
  };

  const up = (e: PointerEvent) => {
    if (dragging) router.dragEnd(dragging, e.clientX, e.clientY, from);
    cleanup();
  };

  const key = (e: KeyboardEvent) => {
    if (e.key === "Escape" && dragging) {
      e.preventDefault();
      e.stopPropagation();
      router.dragCancel();
      cleanup();
    }
  };

  function cleanup() {
    window.removeEventListener("pointermove", move, true);
    window.removeEventListener("pointerup", up, true);
    window.removeEventListener("keydown", key, true);
    ghost?.remove();
    ghost = null;
    document.body.classList.remove("path-dragging");
  }

  window.addEventListener("pointermove", move, true);
  window.addEventListener("pointerup", up, true);
  window.addEventListener("keydown", key, true);
}
