// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * The render-again button in a session clip's bottom-right corner: a
 * curved arrow that renders the inner session's mix as it is now (unsaved
 * edits included when that session is open). The lane draws it and the
 * editor hit-tests it with the same box.
 */

export type Box = { x: number; y: number; w: number; h: number };

/** Where the button sits for a clip drawn from `x0` to `x1`; null when the clip is too small. */
export function rerenderBox(x0: number, x1: number, bottom: number, top: number, width = Infinity): Box | null {
  if (x1 - x0 < 60 || bottom - top < 18) return null;
  const right = Math.min(x1, width);
  if (right - Math.max(x0, 0) < 40) return null;
  return { x: right - 4 - 16, y: bottom - 17, w: 16, h: 14 };
}

/** True when (x, y) is on the button, with a little slack. */
export function onBox(b: Box | null, x: number, y: number): boolean {
  return !!b && x >= b.x - 2 && x <= b.x + b.w + 2 && y >= b.y - 2 && y <= b.y + b.h + 2;
}

/** Draws the button: a rounded tab holding an open circle with an arrowhead (our own glyph). */
export function drawRerender(g: CanvasRenderingContext2D, b: Box, busy = false): void {
  g.save();
  g.fillStyle = "rgba(14, 14, 16, 0.6)";
  g.beginPath();
  g.roundRect(b.x, b.y, b.w, b.h, 3);
  g.fill();
  const cx = b.x + b.w / 2;
  const cy = b.y + b.h / 2;
  const r = 4;
  g.strokeStyle = busy ? "rgba(255, 255, 255, 0.45)" : "rgba(255, 255, 255, 0.95)";
  g.fillStyle = g.strokeStyle;
  g.lineWidth = 1.4;
  g.beginPath();
  g.arc(cx, cy, r, -Math.PI * 0.35, Math.PI * 1.35);
  g.stroke();
  // Arrowhead at the open end, pointing along the turn.
  const a = -Math.PI * 0.35;
  const ex = cx + r * Math.cos(a);
  const ey = cy + r * Math.sin(a);
  g.beginPath();
  g.moveTo(ex + 2.8, ey - 0.6);
  g.lineTo(ex - 0.8, ey - 2.6);
  g.lineTo(ex - 0.2, ey + 1.6);
  g.closePath();
  g.fill();
  g.restore();
}
