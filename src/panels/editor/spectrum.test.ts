// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { fracOf, freqAt, freqTicks, FULL_VIEW, MIN_HZ, MIN_SPAN, rasterize, shapesBounds, SPECTRUM_LUT, viewFrac, viewFreq, viewRange, zoomFreq } from "./spectrum";

describe("spectrum axis", () => {
  it("is log spaced and round-trips", () => {
    expect(freqAt(0, 24_000)).toBeCloseTo(MIN_HZ);
    expect(freqAt(1, 24_000)).toBeCloseTo(24_000);
    for (const hz of [50, 440, 1000, 8000]) expect(freqAt(fracOf(hz, 24_000), 24_000)).toBeCloseTo(hz, 3);
    expect(fracOf(10, 24_000)).toBe(0);
  });

  it("colors run from dark to light", () => {
    expect(SPECTRUM_LUT.length).toBe(768);
    const lum = (i: number) => SPECTRUM_LUT[i * 3]! + SPECTRUM_LUT[i * 3 + 1]! + SPECTRUM_LUT[i * 3 + 2]!;
    expect(lum(0)).toBeLessThan(lum(128));
    expect(lum(128)).toBeLessThan(lum(255));
  });
});

describe("spectral selection masks", () => {
  const at = (m: NonNullable<ReturnType<typeof rasterize>>, t: number, f: number) => {
    const c = Math.floor((t - m.start) / m.spc);
    const r = Math.floor((m.frac_top - f) / m.frac_per_row);
    return c < 0 || r < 0 || c >= m.cols || r >= m.rows ? 0 : m.data[r * m.cols + c]!;
  };

  it("fills a box", () => {
    const m = rasterize([{ kind: "box", start: 1000, end: 2000, bottom: 0.4, top: 0.6 }])!;
    expect(at(m, 1500, 0.5)).toBe(255);
    expect(m.start).toBe(1000);
    expect(m.frac_top).toBeCloseTo(0.6);
  });

  it("fills inside a lasso only", () => {
    const m = rasterize([{ kind: "lasso", points: [[0, 0.2], [1000, 0.2], [1000, 0.8], [500, 0.5], [0, 0.8]] }])!;
    expect(at(m, 100, 0.3)).toBe(255);
    expect(at(m, 500, 0.75)).toBe(0); // the notch
  });

  it("sweeps a brush along its stroke", () => {
    const m = rasterize([{ kind: "brush", points: [[1000, 0.5], [3000, 0.5]], rx: 100, ry: 0.05 }])!;
    expect(at(m, 2000, 0.5)).toBe(255);
    expect(at(m, 2000, 0.54)).toBe(255);
    expect(at(m, 2000, 0.4)).toBe(0);
  });

  it("is null for nothing", () => {
    expect(rasterize([])).toBeNull();
    expect(shapesBounds([{ kind: "lasso", points: [] }])).toBeNull();
  });
});

describe("the spectrum view's frequency axis", () => {
  const ny = 24_000;
  it("matches the edits' log axis when fully zoomed out", () => {
    for (const f of [0, 0.25, 0.5, 1]) expect(viewFreq(f, FULL_VIEW, ny)).toBeCloseTo(freqAt(f, ny), 3);
  });
  it("goes from frequency to place and back, linear or log", () => {
    for (const log of [0, 0.5, 1]) {
      const v = { lo: 100, hi: 8_000, log };
      expect(viewFrac(viewFreq(0.3, v, ny), v, ny)).toBeCloseTo(0.3, 4);
    }
  });
  it("zooms around the pointer, within 0 Hz and the Nyquist frequency", () => {
    const v = zoomFreq(FULL_VIEW, 1_000, 0.5, ny);
    const [lo, hi] = viewRange(v, ny);
    expect(lo).toBeGreaterThan(30);
    expect(hi).toBeLessThan(ny);
    // 1 kHz stays where it was.
    expect(viewFrac(1_000, v, ny)).toBeCloseTo(viewFrac(1_000, FULL_VIEW, ny), 2);
    // Never narrower than the minimum, never past the top.
    let w = v;
    for (let i = 0; i < 60; i++) w = zoomFreq(w, 1_000, 0.5, ny);
    const [a, b] = viewRange(w, ny);
    expect(b - a).toBeGreaterThanOrEqual(MIN_SPAN - 1e-6);
    let out = v;
    for (let i = 0; i < 60; i++) out = zoomFreq(out, 1_000, 2, ny);
    expect(out.hi).toBeNull();
  });
  it("labels the axis with spaced marks", () => {
    const t = freqTicks(FULL_VIEW, ny, 300);
    expect(t).toContain(1_000);
    expect(freqTicks({ lo: 400, hi: 600, log: 0 }, ny, 300).every((f) => f >= 400 && f <= 600)).toBe(true);
  });
});
