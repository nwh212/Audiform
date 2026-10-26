// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import {
  defaultTrackColor,
  dragOverviewEdge,
  fit,
  formatClock,
  formatIn,
  formatTimecode,
  formatTime,
  overviewWindow,
  peakRange,
  pickLevel,
  maxSppFor,
  rulerTicks,
  sampleAt,
  xOf,
  zoomAround,
  MIN_SPP,
} from "./timeline";

describe("timeline geometry", () => {
  it("converts between samples and pixels", () => {
    const v = { spp: 100, scroll: 1000 };
    expect(xOf(1500, v)).toBe(5);
    expect(sampleAt(5, v)).toBe(1500);
    expect(sampleAt(-50, v)).toBe(0);
  });

  it("zooms around the anchor and clamps", () => {
    const v = { spp: 100, scroll: 1000 };
    const z = zoomAround(v, 0.5, 200);
    expect(z.spp).toBe(50);
    expect(sampleAt(200, z)).toBe(sampleAt(200, v));
    expect(zoomAround({ spp: MIN_SPP, scroll: 0 }, 0.1, 0).spp).toBe(MIN_SPP);
    expect(zoomAround({ spp: 100, scroll: 0 }, 2, 500).scroll).toBe(0);
  });

  it("fits a session into the width", () => {
    expect(fit(48000 * 60, 1000).spp).toBeCloseTo((48000 * 60 * 1.05) / 1000);
    expect(fit(0, 1000).spp).toBe(1024);
  });

  it("labels ruler ticks at readable spacing", () => {
    // 1,000 px showing one minute at 48 kHz: labels every 10 s.
    const ticks = rulerTicks({ spp: 2880, scroll: 0 }, 1000, 48000);
    const labels = ticks.filter((t) => t.label).map((t) => t.label);
    expect(labels.slice(0, 3)).toEqual(["0:00", "0:10", "0:20"]);
    const labeled = ticks.filter((t) => t.label);
    expect(labeled[1]!.x - labeled[0]!.x).toBeGreaterThanOrEqual(90);
  });

  it("formats times", () => {
    expect(formatTime(75.399)).toBe("1:15.399");
    expect(formatTime(4505.4, 1)).toBe("1:15:05");
    expect(formatTime(0.5, 0.1)).toBe("0:00.5");
    expect(formatClock(48000 * 4505.399, 48000)).toBe("1:15:05.399");
  });
});

describe("peaks", () => {
  const fine = {
    block: 256,
    data: new Int16Array([-100, 200, -300, 400, -50, 50]),
  };
  const coarse = { block: 4096, data: new Int16Array([-300, 400]) };

  it("picks the coarsest level that still resolves a pixel", () => {
    expect(pickLevel([fine, coarse], 10000)?.block).toBe(4096);
    expect(pickLevel([fine, coarse], 1000)?.block).toBe(256);
    expect(pickLevel([fine, coarse], 10)?.block).toBe(256);
    expect(pickLevel([], 10)).toBeNull();
  });

  it("reads min and max over a range of blocks", () => {
    const [lo, hi] = peakRange(fine, 1, 0, 0, 512);
    expect(lo).toBeCloseTo(-300 / 32767);
    expect(hi).toBeCloseTo(400 / 32767);
    expect(peakRange(fine, 1, 0, 10_000, 20_000)).toEqual([0, 0]);
  });
});

describe("overview strip", () => {
  it("places the window over the visible span", () => {
    expect(
      overviewWindow({ spp: 100, scroll: 10_000 }, 500, 100_000, 1000),
    ).toEqual({ x: 100, w: 500 });
  });

  it("drags an edge to zoom while the other edge stays", () => {
    const v = { spp: 100, scroll: 10_000 };
    const r = dragOverviewEdge(v, "right", 350, 500, 100_000, 1000);
    expect(r.scroll).toBe(10_000);
    expect(r.spp).toBeCloseTo(50);
    const l = dragOverviewEdge(v, "left", 300, 500, 100_000, 1000);
    expect(l.scroll + l.spp * 500).toBeCloseTo(60_000);
    expect(l.spp).toBeCloseTo(60);
  });
});

describe("timecode", () => {
  it("formats hours, minutes, seconds and frames", () => {
    expect(formatTimecode(48_000 * 3725.5, 48_000)).toBe("01:02:05:15");
    expect(formatIn(48_000, 48_000, "samples")).toBe("48000");
    expect(formatIn(48_000, 48_000, "time")).toBe("0:01.000");
  });
});

describe("default track colors", () => {
  it("cycles the clip palette by row", () => {
    expect(defaultTrackColor(0)).toBe("--clip-1");
    expect(defaultTrackColor(5)).toBe("--clip-6");
    expect(defaultTrackColor(6)).toBe("--clip-1");
  });
});

describe("ruler range and zoom limit", () => {
  it("uses coarser labels when zoomed far out, never past a few hours per label", () => {
    // 1,000 px over 4 hours at 48 kHz: about 14.4 s per pixel.
    const ticks = rulerTicks({ spp: 14.4 * 48000, scroll: 0 }, 1000, 48000);
    const labeled = ticks.filter((t) => t.label);
    expect(labeled.length).toBeGreaterThanOrEqual(5);
    expect(labeled[1]!.x - labeled[0]!.x).toBeGreaterThanOrEqual(90);
    expect(ticks.length).toBeGreaterThan(labeled.length);
  });

  it("caps zoom out at 4 hours unless the content is longer", () => {
    const rate = 48000;
    const w = 1000;
    expect(maxSppFor(w, 60 * rate, rate)).toBeCloseTo((4 * 3600 * rate) / w, 0);
    expect(maxSppFor(w, 6 * 3600 * rate, rate)).toBeCloseTo(
      (6 * 3600 * 1.1 * rate) / w,
      0,
    );
    const z = zoomAround(
      { spp: 1000, scroll: 0 },
      1e9,
      0,
      maxSppFor(w, 60 * rate, rate),
    );
    expect(z.spp).toBeCloseTo((4 * 3600 * rate) / w, 0);
  });
});

describe("bars and beats", () => {
  it("shows and reads bar.beat.tick at the tempo", async () => {
    const { formatBars, parseBars, barTicks } = await import("./timeline");
    const t = { bpm: 120, beats_per_bar: 4 };
    // 120 bpm: a beat is half a second.
    expect(formatBars(0, 48_000, t)).toBe("1.1.000");
    expect(formatBars(24_000, 48_000, t)).toBe("1.2.000");
    expect(formatBars(96_000 + 12_000, 48_000, t)).toBe("2.1.480");
    expect(parseBars("2.1.480", 48_000, t)).toBe(108_000);
    expect(parseBars("3", 48_000, t)).toBe(192_000);
    expect(parseBars("0.1", 48_000, t)).toBeNull();
    // A bar is 2 s = 96,000 samples; at 1,000 samples a pixel, bars are 96 px apart and labeled.
    const ticks = barTicks({ spp: 1000, scroll: 0 }, 400, 48_000, t);
    expect(ticks.filter((k) => k.label).map((k) => k.label)).toEqual(["1", "2", "3", "4", "5"]);
  });

  it("follows tempo and time signature changes", async () => {
    const { formatBars, parseBars, barTicks, gridLines } = await import("./timeline");
    // Two bars of 4/4 at 120 (4 s), then 6/8 at 60: an eighth is half a second, a bar 3 s.
    const t = { bpm: 120, beats_per_bar: 4, changes: [{ id: "a", bar: 2, bpm: 60, beats_per_bar: 6, beat_unit: 8 }] };
    const rate = 1_000;
    expect(formatBars(4_000, rate, t)).toBe("3.1.000");
    expect(formatBars(4_500, rate, t)).toBe("3.2.000");
    expect(formatBars(7_000, rate, t)).toBe("4.1.000");
    expect(parseBars("4", rate, t)).toBe(7_000);
    expect(parseBars("3.3", rate, t)).toBe(5_000);
    const labels = barTicks({ spp: 10, scroll: 0 }, 1_000, rate, t).filter((k) => k.label);
    expect(labels.map((k) => [k.label, Math.round(k.x)])).toEqual([
      ["1", 0],
      ["2", 200],
      ["3", 400],
      ["4", 700],
      ["5", 1_000],
    ]);
    const bars = gridLines({ spp: 10, scroll: 0 }, 1_000, rate, t).filter((l) => l.bar);
    expect(bars.map((l) => Math.round(l.x))).toEqual([0, 200, 400, 700, 1_000]);
  });
});
