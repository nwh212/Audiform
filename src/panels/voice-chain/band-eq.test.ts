// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { EffectInfo } from "../../app/core";
import { bandHz, carryBands } from "./band-eq";

const eq = (names: string[]): EffectInfo => ({
  id: "x",
  name: "x",
  category: "",
  description: "",
  params: [
    ...names.map((name, i) => ({ key: `g${i}`, name, unit: "dB", min: -15, max: 15, default: 0, step: 0.1, log: false, toggle: false, group: "Bands" })),
    { key: "output", name: "Output", unit: "dB", min: -15, max: 15, default: 0, step: 0.1, log: false, toggle: false },
  ],
});

describe("band EQ tabs", () => {
  it("reads band frequencies", () => {
    expect(bandHz("31 Hz")).toBe(31);
    expect(bandHz("1.6 kHz")).toBe(1600);
    expect(bandHz("Output")).toBeNull();
  });

  it("keeps the curve's shape and the output when the band count changes", () => {
    const ten = eq(["100 Hz", "1 kHz", "10 kHz"]);
    const more = eq(["100 Hz", "316 Hz", "1 kHz", "3.16 kHz", "10 kHz"]);
    const out = carryBands(ten, { g0: 6, g1: 0, g2: -6, output: 2 }, more);
    expect(out.g0).toBeCloseTo(6);
    expect(out.g1).toBeCloseTo(3);
    expect(out.g2).toBeUndefined();
    expect(out.g3).toBeCloseTo(-3);
    expect(out.g4).toBeCloseTo(-6);
    expect(out.output).toBe(2);
  });
});
