// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { EffectInfo } from "../../app/core";
import { dbAt, freqAt, handles, range, xOf, yOf } from "./graph";

describe("response graph", () => {
  it("maps frequency and level both ways", () => {
    expect(xOf(20)).toBe(0);
    expect(xOf(20_000)).toBeCloseTo(1);
    expect(freqAt(xOf(1000))).toBeCloseTo(1000);
    expect(yOf(24, 24, -24)).toBe(0);
    expect(dbAt(yOf(-6, 24, -24), 24, -24)).toBeCloseTo(-6);
  });

  it("opens the range for deep cuts and finds Curve EQ points", () => {
    expect(range([[100, -3]])).toEqual([24, -24]);
    expect(range([[100, -45]])).toEqual([24, -48]);
    const p = (key: string, unit: string, group: string) => ({ key, name: key, unit, min: -40, max: 20, default: 0, step: 1, log: false, toggle: false, group });
    const info: EffectInfo = { id: "altaudio.curve_eq", name: "Curve EQ", category: "", description: "", params: [p("p1_freq", "Hz", "Point 1"), p("p1_gain", "dB", "Point 1")] };
    expect(handles(info).map((h) => [h.freq.key, h.gain.key])).toEqual([["p1_freq", "p1_gain"]]);
    expect(range([], info)).toEqual([24, -42]);
  });
});
