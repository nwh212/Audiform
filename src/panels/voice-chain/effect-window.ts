// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * The floating settings window for one Audio FX chain effect. Adding an
 * effect (from the panel or the Effects menu) opens it; clicking an
 * effect's name or its open button opens it again.
 */

import { writable } from "svelte/store";

/** Slot id to open in the effect window. The Audio FX panel takes it and clears it. */
export const effectWindowRequest = writable<string | null>(null);

export interface WindowBox {
  x: number;
  y: number;
}

/** Keeps a window of `w` x `h` with its title bar on screen. */
export function clampBox(b: WindowBox, w: number, h: number, vw: number, vh: number): WindowBox {
  const x = Math.min(Math.max(0, b.x), Math.max(0, vw - Math.min(w, vw)));
  const y = Math.min(Math.max(0, b.y), Math.max(0, vh - 32));
  return { x: Math.round(x), y: Math.round(y) };
}

/** Moves an element to the end of the page body, so no panel clips or scrolls it. */
export function portal(node: HTMLElement): { destroy(): void } {
  document.body.appendChild(node);
  return {
    destroy() {
      node.remove();
    },
  };
}
