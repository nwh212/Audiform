// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Left and Right arrows, held, scrub the playhead back and forward at
 * four times normal speed. Stopped, the playhead glides there silently;
 * while playing, playback skips along at that speed so you hear where it
 * is. Letting go leaves the playhead where it got to (and playing goes on
 * from there). Keys typed into a field, a list or a slider, and keys
 * under an open dialog, are left alone.
 */

import { isTextEntry } from "../keymap/dispatch";
import type { TransportStore } from "../stores/transport";

/** How much faster than normal the playhead moves. */
export const SCRUB_SPEED = 4;
/** While playing, how often playback jumps along (ms). */
const HOP_MS = 120;

export interface ScrubTarget {
  transport: TransportStore;
  /** The stopped playhead, in samples. */
  playhead: () => number;
  /** The end of the content, in samples. */
  end: () => number;
  rate: () => number;
}

/** True when an arrow key belongs to the element that has it (a list, slider, menu...). */
function owned(target: EventTarget | null): boolean {
  if (isTextEntry(target)) return true;
  if (!(target instanceof Element)) return false;
  return target.closest('select, input, [role="slider"], [role="listbox"], [role="menu"], [role="menuitem"], [role="tablist"], [role="grid"], [role="tree"]') !== null;
}

/** Listens for held arrows on the window. Returns a function that stops listening. */
export function bindScrub(target: () => ScrubTarget | null): () => void {
  let dir: -1 | 1 | 0 = 0;
  let raf = 0;
  let hop: ReturnType<typeof setInterval> | null = null;
  const stop = () => {
    dir = 0;
    cancelAnimationFrame(raf);
    if (hop) clearInterval(hop);
    hop = null;
  };
  const start = (d: -1 | 1) => {
    const t = target();
    if (!t) return;
    dir = d;
    const rate = t.rate();
    const clamp = (v: number) => Math.max(0, Math.min(t.end(), v));
    if (t.transport.current.playing) {
      // Playback moves on by one step itself; each hop adds the rest.
      const step = (HOP_MS / 1000) * rate;
      hop = setInterval(() => {
        const now = t.transport.current.position;
        void t.transport.seek(clamp(now + (dir > 0 ? (SCRUB_SPEED - 1) * step : -(SCRUB_SPEED + 1) * step)));
      }, HOP_MS);
      return;
    }
    let pos = t.playhead();
    let last = performance.now();
    const tick = (now: number) => {
      if (!dir) return;
      pos = clamp(pos + (dir * SCRUB_SPEED * rate * (now - last)) / 1000);
      last = now;
      void t.transport.seek(pos);
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
  };
  const down = (e: KeyboardEvent) => {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    if (e.ctrlKey || e.altKey || e.metaKey || e.shiftKey || e.defaultPrevented) return;
    if (owned(e.target) || document.querySelector('[aria-modal="true"]')) return;
    e.preventDefault();
    const d = e.key === "ArrowRight" ? 1 : -1;
    if (dir === d) return;
    stop();
    start(d);
  };
  const up = (e: KeyboardEvent) => {
    if ((e.key === "ArrowLeft" && dir === -1) || (e.key === "ArrowRight" && dir === 1)) stop();
  };
  window.addEventListener("keydown", down);
  window.addEventListener("keyup", up);
  window.addEventListener("blur", stop);
  return () => {
    stop();
    window.removeEventListener("keydown", down);
    window.removeEventListener("keyup", up);
    window.removeEventListener("blur", stop);
  };
}
