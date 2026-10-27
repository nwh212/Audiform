// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/** Draws the Multitrack editor's time ruler: ticks, the time selection, markers and the playhead. */

import type { TimeFormat } from "../../stores/editor";
import type { MarkerDoc } from "../../stores/session";
import { KIND_COLORS } from "../markers/markers";
import { RULER_H } from "./editor-layout";
import { formatIn, sampleAt, ticksFor, xOf, type Viewport } from "./timeline";

export interface RulerScene {
  width: number;
  view: Viewport;
  rate: number;
  timeFormat: TimeFormat;
  selection: { start: number; end: number } | null | undefined;
  markers: readonly MarkerDoc[];
  playhead: number;
}

export function drawRuler(c: HTMLCanvasElement, s: RulerScene): void {
  const { view, rate, timeFormat, playhead } = s;
  const dpr = window.devicePixelRatio || 1;
  const w = s.width;
  c.width = Math.round(w * dpr);
  c.height = Math.round(RULER_H * dpr);
  c.style.width = `${w}px`;
  c.style.height = `${RULER_H}px`;
  const g = c.getContext("2d");
  if (!g) return;
  g.setTransform(dpr, 0, 0, dpr, 0, 0);
  const cs = getComputedStyle(document.documentElement);
  g.fillStyle = cs.getPropertyValue("--surface-2");
  g.fillRect(0, 0, w, RULER_H);
  g.fillStyle = cs.getPropertyValue("--text-dim");
  g.strokeStyle = cs.getPropertyValue("--border-strong");
  g.font = `10px ${cs.getPropertyValue("--font-ui")}`;
  g.textBaseline = "top";
  for (const t of ticksFor(view, w, rate, timeFormat)) {
    const x = Math.round(t.x) + 0.5;
    g.beginPath();
    g.moveTo(x, t.label ? 8 : 15);
    g.lineTo(x, RULER_H);
    g.stroke();
    if (t.label) {
      const text = timeFormat === "time" || timeFormat === "bars" ? t.label : formatIn(sampleAt(t.x, view), rate, timeFormat);
      g.fillText(text, x + 3, 2);
    }
  }
  const sel = s.selection;
  if (sel) {
    const a = Math.max(0, xOf(sel.start, view));
    const b = Math.min(w, xOf(sel.end, view));
    if (b > a) {
      g.fillStyle = "rgba(255, 255, 255, 0.16)";
      g.fillRect(a, 0, b - a, RULER_H);
    }
  }
  for (const m of s.markers) {
    const mx = Math.round(xOf(m.position, view));
    const color = m.color || cs.getPropertyValue(KIND_COLORS[m.kind]) || "#c8b040";
    if (m.duration > 0) {
      const ex = xOf(m.position + m.duration, view);
      if (ex >= 0 && mx <= w) {
        g.fillStyle = color;
        g.globalAlpha = 0.35;
        g.fillRect(Math.max(0, mx), 0, Math.min(w, ex) - Math.max(0, mx), 6);
        g.globalAlpha = 1;
      }
    }
    if (mx < -60 || mx > w + 6) continue;
    g.fillStyle = color;
    g.beginPath();
    g.moveTo(mx, 0);
    g.lineTo(mx + 6, 0);
    g.lineTo(mx + 6, 6);
    g.lineTo(mx, 10);
    g.closePath();
    g.fill();
    g.fillRect(mx, 0, 1, RULER_H);
    const tw = g.measureText(m.name).width;
    g.fillStyle = cs.getPropertyValue("--surface-2");
    g.fillRect(mx + 7, 0, tw + 4, 12);
    g.fillStyle = cs.getPropertyValue("--text");
    g.fillText(m.name, mx + 9, 1);
  }
  const px = xOf(playhead, view);
  if (px >= -6 && px <= w + 6) {
    g.fillStyle = cs.getPropertyValue("--playhead");
    // Same pixel column as the line in the lanes, which fills [round(px), round(px) + 1].
    const cx = Math.round(px) + 0.5;
    g.beginPath();
    g.moveTo(cx - 5, RULER_H - 8);
    g.lineTo(cx + 5, RULER_H - 8);
    g.lineTo(cx, RULER_H);
    g.fill();
  }
}
