// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { BAND_EQ_PRESETS, bandCentres, bandEqPresets, bandParams, curveAt } from "./band-presets";
import { BUILTIN_PRESETS } from "./chain";

describe("band EQ presets", () => {
  it("has the core's band centres", () => {
    expect(bandCentres(10)).toHaveLength(10);
    expect(bandCentres(20)).toHaveLength(20);
    expect(bandCentres(30)).toHaveLength(30);
    expect(bandCentres(20)[2]).toBeCloseTo(63);
    expect(bandCentres(20)[19]).toBeCloseTo(31.5 * 2 ** 9.5);
  });

  it("reads a curve evenly in octaves and holds its ends", () => {
    const c = [[100, 0], [400, 6]] as const;
    expect(curveAt(c, 50)).toBe(0);
    expect(curveAt(c, 200)).toBeCloseTo(3);
    expect(curveAt(c, 1000)).toBe(6);
  });

  it("keeps the 10-band settings the first presets had", () => {
    const ten = (name: string) => bandParams(BAND_EQ_PRESETS.find((p) => p.name === name)!.curve, 10);
    expect(ten("Smile Curve")).toEqual({ g01: 4, g02: 3, g03: 1, g05: -1, g06: -1, g08: 1, g09: 3, g10: 4 });
    expect(ten("Bass Boost")).toEqual({ g01: 5, g02: 4, g03: 2, g04: 1 });
    expect(ten("Treble Lift")).toEqual({ g07: 1, g08: 2, g09: 3, g10: 4 });
    expect(ten("Mud Cut")).toEqual({ g03: -3, g04: -3, g05: -1 });
  });

  it("offers every preset on each band count, in range, in the effect's own list", () => {
    const all = bandEqPresets();
    expect(all).toHaveLength(BAND_EQ_PRESETS.length * 3);
    for (const p of all) {
      const keys = Object.keys(p.params);
      expect(keys.length, p.name).toBeGreaterThan(0);
      for (const k of keys) {
        expect(Number(k.slice(1))).toBeLessThanOrEqual(p.bands);
        expect(Math.abs(p.params[k]!)).toBeLessThanOrEqual(15);
      }
      const b = BUILTIN_PRESETS.find((x) => x.name === p.name)!;
      expect(b.chain.slots).toHaveLength(1);
      expect(b.chain.slots[0]!.effect).toBe(`altaudio.${p.effect}`);
    }
    // The same curve on more bands: the 30-band Bass Boost's lowest bands are its lift.
    expect(all.find((p) => p.name === "Bass Boost (30 bands)")!.params.g02).toBe(5);
  });
});
