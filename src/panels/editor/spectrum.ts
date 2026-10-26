// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * The spectrum view's frequency axis and colors. The axis is log spaced
 * from MIN_HZ to the Nyquist frequency and must match
 * `src-tauri/src/dsp/spectral.rs`.
 */

export const MIN_HZ = 30;

/** Frequency at `frac` up the axis (0 bottom, 1 top). */
export function freqAt(frac: number, nyquist: number): number {
  const top = Math.max(nyquist, MIN_HZ * 2);
  return MIN_HZ * Math.pow(top / MIN_HZ, Math.min(1, Math.max(0, frac)));
}

/** How far up the axis `hz` sits, 0 to 1. */
export function fracOf(hz: number, nyquist: number): number {
  const top = Math.max(nyquist, MIN_HZ * 2);
  if (hz <= MIN_HZ) return 0;
  return Math.min(1, Math.log(hz / MIN_HZ) / Math.log(top / MIN_HZ));
}

/**
 * How the spectrum view shows frequency: the range in view (`lo` to `hi`
 * Hz; `hi` null is the Nyquist frequency) and how logarithmic the scale
 * is, from 0 (linear, even in Hz) to 1 (log, even in pitch). Selections
 * are kept on the edits' own log axis (`freqAt`, `fracOf`), so they stay
 * put whatever the view shows.
 */
export interface FreqView {
  lo: number;
  hi: number | null;
  log: number;
}

export const FULL_VIEW: FreqView = { lo: MIN_HZ, hi: null, log: 1 };
/** Lowest frequency a log scale shows. */
const LOG_FLOOR = 10;
/** The narrowest range zooming in goes to, in Hz. */
export const MIN_SPAN = 40;

/** The view's range, settled for a file at `nyquist`. */
export function viewRange(v: FreqView, nyquist: number): [number, number] {
  const hi = Math.min(nyquist, v.hi ?? nyquist);
  const lo = Math.max(0, Math.min(v.lo, hi - MIN_SPAN));
  return [lo, Math.max(hi, lo + MIN_SPAN)];
}

/** Frequency at `frac` up the view (0 bottom, 1 top). */
export function viewFreq(frac: number, v: FreqView, nyquist: number): number {
  const [lo, hi] = viewRange(v, nyquist);
  const t = Math.min(1, Math.max(0, frac));
  const k = Math.min(1, Math.max(0, v.log));
  const l0 = Math.max(lo, LOG_FLOOR);
  const top = Math.max(hi, l0 + 1);
  // A curve that bends from straight (0) to fully logarithmic (1) by
  // degrees: each step moves the middle of the view by a similar number of
  // octaves, rather than staying mostly linear until the very end.
  const a = k * Math.log(top / l0);
  const start = lo + (l0 - lo) * k;
  if (a < 1e-6) return start + (top - start) * t;
  return start + ((top - start) * Math.expm1(a * t)) / Math.expm1(a);
}

/** How far up the view `hz` sits, 0 to 1 (outside the range: below 0 or above 1). */
export function viewFrac(hz: number, v: FreqView, nyquist: number): number {
  const [lo, hi] = viewRange(v, nyquist);
  if (hz <= lo) return hz < lo ? -0.001 : 0;
  if (hz >= hi) return hz > hi ? 1.001 : 1;
  let a = 0;
  let b = 1;
  // The scale only ever rises, so halving finds the spot.
  for (let i = 0; i < 40; i++) {
    const m = (a + b) / 2;
    if (viewFreq(m, v, nyquist) < hz) a = m;
    else b = m;
  }
  return (a + b) / 2;
}

/** Each row's edges, top row first: `rows + 1` frequencies, for the core's spectrogram. */
export function rowEdges(rows: number, v: FreqView, nyquist: number): number[] {
  return Array.from({ length: rows + 1 }, (_, r) => viewFreq(1 - r / rows, v, nyquist));
}

/**
 * Zooms the frequency axis by `factor` (below 1 closer) around `hz`, so
 * that frequency stays where it is. Never past 0 Hz or the Nyquist
 * frequency, never narrower than MIN_SPAN.
 */
export function zoomFreq(v: FreqView, hz: number, factor: number, nyquist: number): FreqView {
  const [lo, hi] = viewRange(v, nyquist);
  const at = Math.min(hi, Math.max(lo, hz));
  let nlo: number;
  let nhi: number;
  if (v.log >= 0.5) {
    // Even in pitch: zoom in ratios.
    const l0 = Math.max(lo, LOG_FLOOR);
    const a = Math.max(at, l0);
    nlo = a * Math.pow(l0 / a, factor);
    nhi = a * Math.pow(hi / a, factor);
    if (lo < LOG_FLOOR && factor >= 1) nlo = Math.min(nlo, lo);
  } else {
    nlo = at - (at - lo) * factor;
    nhi = at + (hi - at) * factor;
  }
  if (nhi - nlo < MIN_SPAN) {
    const mid = Math.min(nyquist - MIN_SPAN / 2, Math.max(MIN_SPAN / 2, (nlo + nhi) / 2));
    nlo = mid - MIN_SPAN / 2;
    nhi = mid + MIN_SPAN / 2;
  }
  nlo = Math.max(v.log >= 0.5 ? Math.min(MIN_HZ, nlo) : 0, nlo);
  nhi = Math.min(nyquist, nhi);
  // Zoomed all the way out: back to the full range.
  const full = nlo <= (v.log >= 0.5 ? MIN_HZ : 0) + 1 && nhi >= nyquist - 1;
  return { ...v, lo: full ? (v.log >= 0.5 ? MIN_HZ : 0) : Math.max(0, nlo), hi: full ? null : nhi };
}

/** Scale marks for the view: well-spaced frequencies with labels. */
export function freqTicks(v: FreqView, nyquist: number, height: number, minGap = 16): number[] {
  const [lo, hi] = viewRange(v, nyquist);
  const nice = [1, 2, 3, 5];
  const out: number[] = [];
  if (v.log >= 0.5) {
    for (let d = 1; d <= 100_000; d *= 10) for (const m of nice) out.push(m * d);
  } else {
    const span = hi - lo;
    const want = Math.max(2, Math.floor(height / (minGap * 2)));
    const raw = span / want;
    const p = Math.pow(10, Math.floor(Math.log10(raw)));
    const step = [1, 2, 5, 10].map((m) => m * p).find((s) => s >= raw) ?? raw;
    for (let f = Math.ceil(lo / step) * step; f <= hi; f += step) out.push(Math.round(f * 1000) / 1000);
  }
  const kept: number[] = [];
  let lastY = Infinity;
  for (const f of out.filter((f) => f > lo && f < hi).sort((a, b) => a - b)) {
    const y = (1 - viewFrac(f, v, nyquist)) * height;
    if (lastY - y >= minGap) {
      kept.push(f);
      lastY = y;
    }
  }
  return kept;
}

/** Color stops from silent to loud: near black, indigo, teal, amber, warm white. */
const STOPS: [number, [number, number, number]][] = [
  [0, [8, 8, 13]],
  [0.3, [42, 26, 94]],
  [0.55, [38, 128, 140]],
  [0.8, [222, 178, 70]],
  [1, [255, 250, 228]],
];

/** 256 RGB triples, flattened. */
export const SPECTRUM_LUT: Uint8Array = (() => {
  const out = new Uint8Array(256 * 3);
  for (let i = 0; i < 256; i++) {
    const t = i / 255;
    let k = 0;
    while (k < STOPS.length - 2 && t > STOPS[k + 1]![0]) k++;
    const [t0, c0] = STOPS[k]!;
    const [t1, c1] = STOPS[k + 1]!;
    const u = (t - t0) / (t1 - t0);
    for (let j = 0; j < 3; j++) out[i * 3 + j] = Math.round(c0[j]! + (c1[j]! - c0[j]!) * u);
  }
  return out;
})();

/** One part of a spectral selection. Times are samples; heights are axis positions (0 bottom, 1 top). */
export type SpectralShape =
  | { kind: "box"; start: number; end: number; bottom: number; top: number }
  | { kind: "lasso"; points: [number, number][] }
  | { kind: "brush"; points: [number, number][]; rx: number; ry: number };

/** What the core's spectral edit takes: weights 0 to 255 over time and the axis. */
export interface SpectralMask {
  start: number;
  spc: number;
  cols: number;
  rows: number;
  frac_top: number;
  frac_per_row: number;
  data: number[];
}

/** Time and axis extent of shapes. */
export function shapesBounds(shapes: readonly SpectralShape[]): { t0: number; t1: number; f0: number; f1: number } | null {
  let t0 = Infinity;
  let t1 = -Infinity;
  let f0 = Infinity;
  let f1 = -Infinity;
  for (const s of shapes) {
    if (s.kind === "box") {
      t0 = Math.min(t0, s.start);
      t1 = Math.max(t1, s.end);
      f0 = Math.min(f0, s.bottom);
      f1 = Math.max(f1, s.top);
    } else {
      const rx = s.kind === "brush" ? s.rx : 0;
      const ry = s.kind === "brush" ? s.ry : 0;
      for (const [t, f] of s.points) {
        t0 = Math.min(t0, t - rx);
        t1 = Math.max(t1, t + rx);
        f0 = Math.min(f0, f - ry);
        f1 = Math.max(f1, f + ry);
      }
    }
  }
  if (!(t1 > t0) || !(f1 > f0)) return null;
  return { t0: Math.max(0, t0), t1, f0: Math.max(0, f0), f1: Math.min(1, f1) };
}

function inPolygon(x: number, y: number, pts: readonly [number, number][]): boolean {
  let inside = false;
  for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) {
    const [xi, yi] = pts[i]!;
    const [xj, yj] = pts[j]!;
    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}

/** Turns shapes into a weight grid for the core. Null when nothing is selected. */
export function rasterize(shapes: readonly SpectralShape[], maxCols = 800, maxRows = 300): SpectralMask | null {
  const b = shapesBounds(shapes);
  if (!b) return null;
  const cols = Math.max(1, Math.min(maxCols, Math.ceil(b.t1 - b.t0)));
  const spc = (b.t1 - b.t0) / cols;
  const rows = Math.max(1, Math.min(maxRows, Math.ceil((b.f1 - b.f0) * 1000)));
  const fpr = (b.f1 - b.f0) / rows;
  const data = new Array<number>(cols * rows).fill(0);
  const tAt = (c: number) => b.t0 + (c + 0.5) * spc;
  const fAt = (r: number) => b.f1 - (r + 0.5) * fpr;
  for (const s of shapes) {
    if (s.kind === "box") {
      const c0 = Math.max(0, Math.floor((s.start - b.t0) / spc));
      const c1 = Math.min(cols - 1, Math.ceil((s.end - b.t0) / spc) - 1);
      const r0 = Math.max(0, Math.floor((b.f1 - s.top) / fpr));
      const r1 = Math.min(rows - 1, Math.ceil((b.f1 - s.bottom) / fpr) - 1);
      for (let r = r0; r <= r1; r++) for (let c = c0; c <= c1; c++) data[r * cols + c] = 255;
    } else if (s.kind === "lasso") {
      if (s.points.length < 3) continue;
      for (let r = 0; r < rows; r++) for (let c = 0; c < cols; c++) if (inPolygon(tAt(c), fAt(r), s.points)) data[r * cols + c] = 255;
    } else {
      // Brush: an ellipse swept along each segment.
      const pts = s.points.length === 1 ? [s.points[0]!, s.points[0]!] : s.points;
      for (let i = 0; i + 1 < pts.length; i++) {
        const [ax, ay] = pts[i]!;
        const [bx, by] = pts[i + 1]!;
        const c0 = Math.max(0, Math.floor((Math.min(ax, bx) - s.rx - b.t0) / spc));
        const c1 = Math.min(cols - 1, Math.ceil((Math.max(ax, bx) + s.rx - b.t0) / spc));
        const r0 = Math.max(0, Math.floor((b.f1 - Math.max(ay, by) - s.ry) / fpr));
        const r1 = Math.min(rows - 1, Math.ceil((b.f1 - Math.min(ay, by) + s.ry) / fpr));
        // Work in units where the brush is a unit circle.
        const ux = (bx - ax) / s.rx;
        const uy = (by - ay) / s.ry;
        const len2 = ux * ux + uy * uy;
        for (let r = r0; r <= r1; r++) {
          for (let c = c0; c <= c1; c++) {
            const px = (tAt(c) - ax) / s.rx;
            const py = (fAt(r) - ay) / s.ry;
            const t = len2 > 0 ? Math.max(0, Math.min(1, (px * ux + py * uy) / len2)) : 0;
            const dx = px - t * ux;
            const dy = py - t * uy;
            if (dx * dx + dy * dy <= 1) data[r * cols + c] = 255;
          }
        }
      }
    }
  }
  if (!data.some((v) => v > 0)) return null;
  return { start: b.t0, spc, cols, rows, frac_top: b.f1, frac_per_row: fpr, data };
}
