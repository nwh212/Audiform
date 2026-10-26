// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { EffectSlotDoc } from "../../stores/session";
import { addPoint, curveAt, curvePoints, groupShown, removePoint } from "./phase-curve";

const slot = (params: Record<string, number>): EffectSlotDoc => ({ id: "s", effect: "altaudio.phase_curve", enabled: true, params, preset: null });

describe("Phase Curve", () => {
  it("draws the same curve as the core", () => {
    const pts = [
      { index: 0, hz: 100, deg: 0 },
      { index: 1, hz: 1_000, deg: 90 },
      { index: 2, hz: 10_000, deg: -90 },
    ];
    for (const spline of [false, true]) {
      expect(curveAt(pts, spline, 1_000)).toBeCloseTo(90, 9);
      expect(curveAt(pts, spline, 50)).toBe(0);
      expect(curveAt(pts, spline, 20_000)).toBe(-90);
    }
    expect(curveAt(pts, false, 316.227766)).toBeCloseTo(45, 3);
  });

  it("lists the points in use by frequency, within the range", () => {
    const p = curvePoints(slot({ points: 3, p1_freq: 5_000, p1_phase: 300, p2_freq: 100, p3_freq: 1_000 }));
    expect(p.map((x) => x.hz)).toEqual([100, 1_000, 5_000]);
    expect(p[2]!.deg).toBe(180);
    expect(p[2]!.index).toBe(0);
  });

  it("adds and removes points", () => {
    expect(addPoint(slot({ points: 2 }), 700, 45)).toEqual({ points: 3, p3_freq: 700, p3_phase: 45 });
    expect(addPoint(slot({ points: 8 }), 700, 45)).toBeNull();
    // Removing point 1 of 3: point 3 takes its place.
    expect(removePoint(slot({ points: 3, p3_freq: 900, p3_phase: 10 }), 0)).toEqual({ points: 2, p1_freq: 900, p1_phase: 10 });
    expect(removePoint(slot({ points: 2 }), 0)).toBeNull();
  });

  it("shows only the controls of points in use", () => {
    const s = slot({ points: 3 });
    expect(groupShown("altaudio.phase_curve", s, "Point 3")).toBe(true);
    expect(groupShown("altaudio.phase_curve", s, "Point 4")).toBe(false);
    expect(groupShown("altaudio.phase_curve", s, "")).toBe(true);
    expect(groupShown("altaudio.gain", s, "Point 4")).toBe(true);
  });
});
