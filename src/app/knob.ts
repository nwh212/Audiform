// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/** Dial maths, kept apart from the component so it can be tested. */

/** Pixels of vertical drag for the dial's whole range. */
export const DRAG_PIXELS = 160;

const clamp01 = (t: number) => Math.min(1, Math.max(0, t));

/**
 * The dial's value after dragging `dy` pixels up (negative is down) from
 * `start`. Ctrl moves in fine steps, Shift in coarse ones.
 */
export function dragValue(start: number, dy: number, mods: { ctrl?: boolean; shift?: boolean } = {}): number {
  const scale = mods.ctrl ? 0.2 : mods.shift ? 3 : 1;
  return clamp01(start + (dy / DRAG_PIXELS) * scale);
}

/** The dial's value after a key press, or null when the key does nothing. */
export function keyValue(value: number, key: string, home: number): number | null {
  switch (key) {
    case "ArrowUp":
    case "ArrowRight":
      return clamp01(value + 0.01);
    case "ArrowDown":
    case "ArrowLeft":
      return clamp01(value - 0.01);
    case "PageUp":
      return clamp01(value + 0.1);
    case "PageDown":
      return clamp01(value - 0.1);
    case "Home":
      return home;
    default:
      return null;
  }
}

/** Where the dial's pointer points, in degrees from straight up. */
export function pointerAngle(value: number): number {
  return -135 + clamp01(value) * 270;
}
