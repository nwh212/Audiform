// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { EffectInfo } from "../../app/core";
import { eqNodes, formatHz, octavesOf, qFromEdge, qOf, widthEdges } from "./eq-graph";

const p = (key: string, unit: string, group: string, toggle = false) => ({ key, name: key, unit, min: 0, max: 1, default: 0, step: 1, log: false, toggle, retimes: false, group });
const info = {
  id: "altaudio.parametric_eq",
  name: "Parametric EQ",
  category: "Filter and EQ",
  description: "",
  params: [p("hp_on", "", "High-pass", true), p("hp_freq", "Hz", "High-pass"), p("b1_freq", "Hz", "Band 1"), p("b1_gain", "dB", "Band 1"), p("b1_q", "", "Band 1"), p("hs_freq", "Hz", "High shelf"), p("hs_gain", "dB", "High shelf")],
} as unknown as EffectInfo;

describe("the EQ graph", () => {
  it("makes a node per band, with the controls it moves", () => {
    const n = eqNodes(info);
    expect(n.map((x) => x.label)).toEqual(["HP", "1", "H"]);
    expect(n[0]!.on?.key).toBe("hp_on");
    expect(n[0]!.gain).toBeNull();
    expect([n[1]!.gain?.key, n[1]!.q?.key]).toEqual(["b1_gain", "b1_q"]);
  });

  it("turns Q into a width and back", () => {
    // Q 1.414 is about one octave wide.
    expect(octavesOf(Math.SQRT2)).toBeCloseTo(1, 2);
    for (const q of [0.3, 1, 4]) expect(qOf(octavesOf(q))).toBeCloseTo(q, 6);
    const [lo, hi] = widthEdges(1000, Math.SQRT2);
    expect(lo).toBeCloseTo(1000 / Math.SQRT2, 0);
    expect(qFromEdge(1000, hi)).toBeCloseTo(Math.SQRT2, 3);
  });

  it("writes frequencies plainly", () => {
    expect(formatHz(80)).toBe("80 Hz");
    expect(formatHz(1250)).toBe("1.25 kHz");
  });
});
