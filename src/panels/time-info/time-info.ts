// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Time Info's editable times: reading typed times in the display format,
 * and the view changes behind its View row.
 */

import { writable } from "svelte/store";
import { getDisplayTempo, parseBars } from "../editor/timeline";
import type { ViewDoc } from "../../stores/session";

/** Width in pixels of each editor's lanes, set by the editors, so Time Info can show and set the visible range. */
export const laneWidths = writable<{ multitrack: number; waveform: number }>({ multitrack: 0, waveform: 0 });

/**
 * A typed time in samples. Reads "1:05.250", "65.25", "0:01:05" and,
 * in the samples format, a plain count; in timecode, "00:01:05:12" at
 * `fps` too. Null when it is not a time.
 */
export function parseTime(text: string, rate: number, format: "time" | "samples" | "timecode" | "seconds" | "bars", fps: number): number | null {
  const t = text.trim();
  if (format === "bars") {
    const b = parseBars(t, rate, getDisplayTempo());
    if (b !== null) return b;
  }
  if (format === "samples" && /^\d+$/.test(t)) return Number(t);
  if (format === "timecode") {
    const m = /^(\d+):(\d{1,2}):(\d{1,2})[:;](\d{1,2})$/.exec(t);
    if (m) return Math.round(((Number(m[1]) * 60 + Number(m[2])) * 60 + Number(m[3]) + Number(m[4]) / fps) * rate);
  }
  if (!/^\d+(:\d{1,2}){0,2}(\.\d*)?$/.test(t)) return null;
  let secs = 0;
  for (const p of t.split(":")) secs = secs * 60 + Number(p);
  return Number.isFinite(secs) ? Math.round(secs * rate) : null;
}

/** The visible range for a view `width` pixels wide. */
export function viewRange(v: ViewDoc, width: number): { start: number; end: number } {
  return { start: v.scroll, end: v.scroll + v.samples_per_pixel * Math.max(1, width) };
}

/** A view showing `start..end` across `width` pixels. */
export function viewFor(v: ViewDoc, start: number, end: number, width: number): ViewDoc {
  const s = Math.max(0, Math.round(start));
  const len = Math.max(1, end - s);
  return { ...v, scroll: s, samples_per_pixel: Math.max(1 / 64, len / Math.max(1, width)) };
}

/** A selection with one edge or the length changed, kept in order and at or after 0. */
export function editSelection(sel: { start: number; end: number }, part: "start" | "end" | "duration", value: number): { start: number; end: number } {
  const v = Math.max(0, Math.round(value));
  if (part === "duration") return { start: sel.start, end: sel.start + v };
  const a = part === "start" ? v : sel.start;
  const b = part === "end" ? v : sel.end;
  return { start: Math.min(a, b), end: Math.max(a, b) };
}
