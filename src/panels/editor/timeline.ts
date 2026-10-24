// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

/**
 * Timeline geometry for the multitrack view. Pure functions, so the
 * math is tested without a screen.
 */

import { settings } from "../../stores/settings";
export interface Viewport {
  /** Session samples per screen pixel. */
  spp: number;
  /** Session sample at the left edge. */
  scroll: number;
}

/** Closest zoom in: 32 pixels per sample, so single samples can be seen (drawn as points on a line). */
export const MIN_SPP = 1 / 32;
/** Closest zoom in the Waveform editor, the same as the Multitrack's. */
export const SAMPLE_SPP = MIN_SPP;
/** Farthest zoom out: about 6 hours across 1,000 pixels at 48 kHz. */
export const MAX_SPP = 1_048_576;

export function xOf(sample: number, v: Viewport): number {
  return (sample - v.scroll) / v.spp;
}

export function sampleAt(x: number, v: Viewport): number {
  return Math.max(0, Math.round(v.scroll + x * v.spp));
}

/** The timeline shows at most this long unless the content is longer. */
export const DEFAULT_MAX_SECONDS = 4 * 3600;

/** Farthest zoom out for a view `width` pixels wide: 4 hours, or the content (plus 10%) when longer. */
export function maxSppFor(
  width: number,
  contentEnd: number,
  rate: number,
): number {
  const span = Math.max(DEFAULT_MAX_SECONDS * rate, contentEnd * 1.1);
  return Math.min(MAX_SPP * 8, Math.max(MIN_SPP, span / Math.max(1, width)));
}

/** Zooms by `factor` (above 1 zooms out) keeping the sample under `anchorX` in place. */
export function zoomAround(
  v: Viewport,
  factor: number,
  anchorX: number,
  maxSpp = MAX_SPP,
  minSpp = MIN_SPP,
): Viewport {
  const spp = Math.min(maxSpp, Math.max(minSpp, v.spp * factor));
  const anchor = v.scroll + anchorX * v.spp;
  return { spp, scroll: Math.max(0, anchor - anchorX * spp) };
}

/** Fits `length` samples into `width` pixels with a little room at the end. */
export function fit(length: number, width: number): Viewport {
  if (length <= 0 || width <= 0) return { spp: 1024, scroll: 0 };
  return {
    spp: Math.min(MAX_SPP, Math.max(MIN_SPP, (length * 1.05) / width)),
    scroll: 0,
  };
}

/** Seconds between labeled ruler ticks, from fine to coarse. */
const STEPS = [
  0.0001, 0.0002, 0.0005, 0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1, 2, 5, 10, 15, 20, 30,
  60, 120, 180, 300, 600, 900, 1200, 1800, 3600, 7200,
];

/** Unlabeled ticks between labels, where 5 would land on odd times. */
const MINOR_DIVISIONS: Record<number, number> = {
  2: 4,
  15: 3,
  20: 4,
  30: 3,
  60: 4,
  120: 4,
  180: 3,
  900: 3,
  1200: 4,
  1800: 3,
  3600: 4,
  7200: 4,
};

export interface Tick {
  x: number;
  label: string | null;
}

/** Ruler ticks for the visible range. Labeled ticks are at least `minGap` pixels apart. */
export function rulerTicks(
  v: Viewport,
  width: number,
  rate: number,
  minGap = 90,
): Tick[] {
  const pxPerSec = rate / v.spp;
  const step = STEPS.find((s) => s * pxPerSec >= minGap) ?? 14400;
  const minor = step / (MINOR_DIVISIONS[step] ?? 5);
  const startSec = v.scroll / rate;
  const endSec = startSec + (width * v.spp) / rate;
  const ticks: Tick[] = [];
  const first = Math.floor(startSec / minor) * minor;
  // Count in minor steps to avoid floating point drift.
  const n = Math.ceil((endSec - first) / minor) + 1;
  const perMajor = Math.round(step / minor);
  const firstIndex = Math.round(first / minor);
  for (let i = 0; i <= n; i++) {
    const k = firstIndex + i;
    const t = k * minor;
    const x = (t * rate - v.scroll) / v.spp;
    if (x < -1 || x > width + 1) continue;
    ticks.push({ x, label: k % perMajor === 0 ? formatTime(t, step) : null });
  }
  return ticks;
}

/** Time for display. Shows milliseconds only when the step needs them. */
export function formatTime(seconds: number, step = 0.001): string {
  const neg = seconds < 0;
  let s = Math.abs(seconds);
  const decimals = step < 0.001 ? 4 : step < 0.01 ? 3 : step < 0.1 ? 2 : step < 1 ? 1 : 0;
  const scale = 10 ** decimals;
  s = Math.round(s * scale) / scale;
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const secText =
    decimals > 0
      ? sec.toFixed(decimals).padStart(decimals + 3, "0")
      : String(Math.floor(sec)).padStart(2, "0");
  const body =
    h > 0 ? `${h}:${String(m).padStart(2, "0")}:${secText}` : `${m}:${secText}`;
  return neg ? `-${body}` : body;
}

/** Full-precision time for readouts: 1:15:05.399 or 0:42.120. */
export function formatClock(samples: number, rate: number): string {
  return formatTime(samples / rate, 0.001);
}

/** Frames per second for timecode display. Fixed for now; video work mostly uses 30. */
export const TIMECODE_FPS = 30;

/** Timecode as hh:mm:ss:ff (non-drop frame). */
export function formatTimecode(
  samples: number,
  rate: number,
  fps = TIMECODE_FPS,
): string {
  const neg = samples < 0;
  const totalFrames = Math.floor((Math.abs(samples) * fps) / rate);
  const f = totalFrames % fps;
  const totalSec = Math.floor(totalFrames / fps);
  const two = (n: number) => String(n).padStart(2, "0");
  const body = `${two(Math.floor(totalSec / 3600))}:${two(Math.floor((totalSec % 3600) / 60))}:${two(totalSec % 60)}:${two(f)}`;
  return neg ? `-${body}` : body;
}

/** A position in the chosen display format. */
export function formatIn(
  samples: number,
  rate: number,
  format: "time" | "samples" | "timecode",
): string {
  if (format === "samples") return String(Math.round(samples));
  if (format === "timecode")
    return formatTimecode(samples, rate, settings.current.timecodeFps);
  return formatClock(samples, rate);
}

export interface PeakLevel {
  /** Source frames per block. */
  block: number;
  /** min, max per channel per block. */
  data: Int16Array;
}

/**
 * Picks the coarsest level whose blocks are no wider than one pixel's
 * worth of source frames, so each column reads only a few blocks.
 */
export function pickLevel(
  levels: readonly PeakLevel[],
  framesPerPixel: number,
): PeakLevel | null {
  const sorted = [...levels].sort((x, y) => x.block - y.block);
  let best = sorted[0] ?? null;
  for (const l of sorted) if (l.block <= framesPerPixel) best = l;
  return best;
}

/** Min and max (as -1..1) of channel `ch` over source frames [from, to). */
export function peakRange(
  level: PeakLevel,
  channels: number,
  ch: number,
  from: number,
  to: number,
): [number, number] {
  const stride = channels * 2;
  const blocks = level.data.length / stride;
  let b0 = Math.max(0, Math.floor(from / level.block));
  const b1 = Math.min(blocks, Math.max(b0 + 1, Math.ceil(to / level.block)));
  if (b0 >= blocks) return [0, 0];
  let lo = 32767;
  let hi = -32768;
  for (; b0 < b1; b0++) {
    const i = b0 * stride + ch * 2;
    const mn = level.data[i]!;
    const mx = level.data[i + 1]!;
    if (mn < lo) lo = mn;
    if (mx > hi) hi = mx;
  }
  if (lo > hi) return [0, 0];
  return [lo / 32767, hi / 32767];
}

/**
 * Overview strip: the whole session squeezed into `stripW` pixels, with
 * a window showing what the lanes show. `total` is the span the strip
 * covers in session samples.
 */
export function overviewWindow(
  v: Viewport,
  lanesW: number,
  total: number,
  stripW: number,
): { x: number; w: number } {
  const k = stripW / Math.max(1, total);
  return { x: v.scroll * k, w: Math.max(6, lanesW * v.spp * k) };
}

/**
 * New view after dragging an edge of the overview window to `x`
 * (strip pixels). The opposite edge stays put, which zooms the lanes.
 */
export function dragOverviewEdge(
  v: Viewport,
  edge: "left" | "right",
  x: number,
  lanesW: number,
  total: number,
  stripW: number,
): Viewport {
  const perPx = total / Math.max(1, stripW);
  const start = v.scroll;
  const end = v.scroll + lanesW * v.spp;
  const at = Math.max(0, x * perPx);
  const minSpan = MIN_SPP * lanesW;
  let a = start;
  let b = end;
  if (edge === "left") a = Math.min(at, end - minSpan);
  else b = Math.max(at, start + minSpan);
  const spp = Math.min(
    MAX_SPP,
    Math.max(MIN_SPP, (b - a) / Math.max(1, lanesW)),
  );
  return { spp, scroll: Math.max(0, edge === "left" ? b - spp * lanesW : a) };
}

/** Height of the dark break drawn under each multitrack lane, in pixels. */
export const TRACK_GAP = 3;

/** Color for a track that has none of its own: the clip palette, by row. */
export function defaultTrackColor(row: number): string {
  return `--clip-${(((row % 6) + 6) % 6) + 1}`;
}

/** Height of a folded track: its name row. */
export const COLLAPSED_H = 30;

/** A track's height on screen: folded tracks show only their name row. */
export function trackHeight(t: {
  height: number;
  collapsed?: boolean;
}): number {
  return t.collapsed ? COLLAPSED_H : t.height;
}
