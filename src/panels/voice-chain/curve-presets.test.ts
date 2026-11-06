// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { curvePoints, DYNAMICS_CURVE_PRESETS } from "./curve-presets";

describe("Dynamics Curve presets", () => {
  it("turns a segment table into points that run through the first level on the diagonal", () => {
    // 2.5:1 above -15 dB, 2:1 (lifting) below it.
    expect(curvePoints([["c", 2.5, -15], ["c", 2, -15]])).toEqual([[-100, -57.5], [-15, -15], [0, -9]]);
  });

  it("closes off below a steep expander and keeps neutral stretches straight", () => {
    const pts = curvePoints([["n", 1, -20], ["e", 7, -20], ["x", 1, -60]]);
    expect(pts[0]![0]).toBeLessThan(-60);
    expect(pts[0]![1]).toBeLessThanOrEqual(-100);
    expect(pts.at(-1)).toEqual([0, 0]);
  });

  it("every preset has at most eight points, in rising order, inside the controls' ranges", () => {
    for (const p of DYNAMICS_CURVE_PRESETS) {
      const n = p.params["points"]!;
      expect(n, p.name).toBeGreaterThanOrEqual(2);
      expect(n, p.name).toBeLessThanOrEqual(8);
      let prev = -Infinity;
      for (let i = 1; i <= n; i++) {
        const x = p.params[`p${i}_in`]!;
        const y = p.params[`p${i}_out`]!;
        expect(x, `${p.name} point ${i}`).toBeGreaterThan(prev);
        expect(x).toBeGreaterThanOrEqual(-120);
        expect(x).toBeLessThanOrEqual(0);
        expect(y).toBeGreaterThanOrEqual(-120);
        expect(y).toBeLessThanOrEqual(12);
        prev = x;
      }
    }
  });

  it("names are unique", () => {
    const names = DYNAMICS_CURVE_PRESETS.map((p) => p.name.toLowerCase());
    expect(new Set(names).size).toBe(names.length);
  });
});
