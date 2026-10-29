// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { Analyser, BANDS, levelOf } from "./analysis";
import { paletteFor } from "./palette";
import { PRESETS, presetById } from "./presets";

describe("visualizer analysis", () => {
  it("maps dB to 0..1", () => {
    expect(levelOf(-120, 1)).toBe(0);
    expect(levelOf(Number.NaN, 1)).toBe(0);
    expect(levelOf(0, 1)).toBe(1);
    expect(levelOf(-30, 1)).toBeGreaterThan(levelOf(-60, 1));
  });

  it("calls a jump in the bass a beat, once", () => {
    const a = new Analyser();
    const quiet = new Float32Array(BANDS).fill(-80);
    const loud = new Float32Array(BANDS).fill(-80);
    for (let i = 0; i < 10; i++) loud[i] = -6;
    for (let i = 0; i < 20; i++) a.feed(quiet, 1, 0.033);
    expect(a.feed(loud, 1, 0.033).beat).toBe(true);
    expect(a.feed(loud, 1, 0.033).beat).toBe(false);
  });

  it("reads silence from an empty spectrum", () => {
    const r = new Analyser().feed(new Float32Array(0), 1, 0.03);
    expect(r.level).toBe(0);
    expect(r.beat).toBe(false);
  });
});

describe("visualizer looks", () => {
  it("lists distinct looks and falls back to the first", () => {
    expect(new Set(PRESETS.map((p) => p.id)).size).toBe(PRESETS.length);
    expect(presetById("nope").id).toBe(PRESETS[0]!.id);
    expect(PRESETS.some((p) => p.name === "Lava Lamp")).toBe(true);
  });

  it("gives every color scheme a color for any position", () => {
    for (const s of ["theme", "rainbow", "fire", "ocean", "aurora"] as const) {
      const pal = paletteFor(s, null, null);
      for (const t of [-0.3, 0, 0.5, 1, 7.25]) {
        const c = pal(t);
        expect(c.every((v) => Number.isFinite(v) && v >= -1 && v <= 256)).toBe(true);
      }
    }
  });
});
