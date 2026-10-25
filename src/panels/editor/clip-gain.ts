// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

/**
 * The level handle in each clip's bottom-left corner: drag it up or to
 * the right to raise the clip's level, down or to the left to lower it.
 * The lane draws it (LaneCanvas, MidiLane) and the editor hit-tests it
 * with the same box.
 */

/** Lowest and highest clip level, dB (the core clamps to the same). */
export const GAIN_MIN = -96;
export const GAIN_MAX = 24;

/** dB per pixel of drag, and with Shift for fine steps. */
const STEP = 0.1;
const FINE = 0.02;

/**
 * Where the handle sits for a clip drawn from `x0` to `x1` in a lane whose
 * clip body ends at `bottom`. Null when the clip is too small to hold it.
 */
export function gainBox(x0: number, x1: number, bottom: number, top: number): { x: number; y: number; w: number; h: number } | null {
  if (x1 - x0 < 40 || bottom - top < 30) return null;
  return { x: Math.max(x0, 0) + 4, y: bottom - 16, w: 14, h: 12 };
}

/** The level after dragging `dx` right and `dy` down from `start` dB. */
export function dragGain(start: number, dx: number, dy: number, fine: boolean): number {
  const v = start + (dx - dy) * (fine ? FINE : STEP);
  return Math.round(Math.min(GAIN_MAX, Math.max(GAIN_MIN, v)) * 10) / 10;
}

/** "+3.5 dB", "-12.0 dB", or "" at 0. */
export function gainText(db: number): string {
  return db === 0 ? "" : `${db > 0 ? "+" : ""}${db.toFixed(1)} dB`;
}

/**
 * Draws the handle: a small rounded tab with an up-and-down arrow, our own
 * glyph, and the level beside it when it is not 0.
 */
export function drawGainHandle(g: CanvasRenderingContext2D, box: { x: number; y: number; w: number; h: number }, db: number, font: string, hot = false): void {
  g.save();
  g.fillStyle = hot ? "rgba(255, 255, 255, 0.85)" : "rgba(14, 14, 16, 0.5)";
  g.beginPath();
  g.roundRect(box.x, box.y, box.w, box.h, 3);
  g.fill();
  const cx = box.x + box.w / 2;
  g.strokeStyle = hot ? "#0e0e10" : "rgba(255, 255, 255, 0.9)";
  g.lineWidth = 1.2;
  g.beginPath();
  g.moveTo(cx, box.y + 2.5);
  g.lineTo(cx, box.y + box.h - 2.5);
  g.moveTo(cx - 2.5, box.y + 5);
  g.lineTo(cx, box.y + 2.5);
  g.lineTo(cx + 2.5, box.y + 5);
  g.moveTo(cx - 2.5, box.y + box.h - 5);
  g.lineTo(cx, box.y + box.h - 2.5);
  g.lineTo(cx + 2.5, box.y + box.h - 5);
  g.stroke();
  const t = gainText(db);
  if (t) {
    g.font = font;
    g.textBaseline = "middle";
    const w = g.measureText(t).width;
    g.fillStyle = "rgba(14, 14, 16, 0.55)";
    g.beginPath();
    g.roundRect(box.x + box.w + 3, box.y, w + 8, box.h, 3);
    g.fill();
    g.fillStyle = "#f2f2f2";
    g.fillText(t, box.x + box.w + 7, box.y + box.h / 2 + 0.5);
  }
  g.restore();
}
