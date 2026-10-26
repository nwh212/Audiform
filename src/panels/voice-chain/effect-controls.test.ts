// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { EffectParamInfo } from "../../app/core";
import { controlColumns, parseParam } from "./effect-controls";

const p = (key: string, extra: Partial<EffectParamInfo> = {}): EffectParamInfo => ({
  key,
  name: key,
  unit: "",
  min: 0,
  max: 1,
  default: 0,
  step: 0,
  log: false,
  toggle: false,
  ...extra,
});

describe("effect controls", () => {
  it("makes a column per group, with a group's On switch in its heading", () => {
    const on = p("hp_on", { name: "On", toggle: true, group: "High-pass" });
    const cols = controlColumns([p("mix"), on, p("hp_freq", { group: "High-pass" }), p("b1_gain", { group: "Band 1" }), p("print", { hidden: true })]);
    expect(cols.map((c) => c.name)).toEqual(["", "High-pass", "Band 1"]);
    expect(cols[1]!.power).toBe(on);
    expect(cols[1]!.params.map((x) => x.key)).toEqual(["hp_freq"]);
  });

  it("reads typed values in the control's unit", () => {
    const hz = p("f", { unit: "Hz", min: 20, max: 20_000, step: 1 });
    expect(parseParam(hz, "1.5k")).toBe(1500);
    expect(parseParam(hz, "2 kHz")).toBe(2000);
    expect(parseParam(hz, "80 Hz")).toBe(80);
    expect(parseParam(hz, "5")).toBe(20);
    const db = p("g", { unit: "dB", min: -24, max: 24, step: 0.1 });
    expect(parseParam(db, "-6.34 dB")).toBe(-6.3);
    expect(parseParam(db, "3,5")).toBe(3.5);
    expect(parseParam(db, "loud")).toBeNull();
  });
});
