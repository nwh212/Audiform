// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { ClipDoc, EnvelopePointDoc } from "../../stores/session";
import { addPoint, dbToY, envChange, formatValue, fxChoices, fxLaneOf, levelAt, movePoint, pointsOf, rangeOf, restValue, setFxCatalog, snapValue, sliceVolume, valueToY, yToDb, yToValue } from "./envelope";

const pt = (position: number, value: number): EnvelopePointDoc => ({ position, value, curve: "linear" });

describe("volume envelope", () => {
  const pts = [pt(100, 0), pt(300, -20)];

  it("holds before and after, and interpolates in dB", () => {
    expect(levelAt(pts, 0)).toBe(0);
    expect(levelAt(pts, 200)).toBe(-10);
    expect(levelAt(pts, 1_000)).toBe(-20);
    expect(levelAt([], 5)).toBe(0);
  });

  it("maps dB to the lane and back", () => {
    expect(dbToY(12, 10, 72)).toBe(10);
    expect(dbToY(-60, 10, 72)).toBe(82);
    expect(yToDb(dbToY(-6, 0, 100), 0, 100)).toBe(-6);
  });

  it("slices a piece and keeps its levels", () => {
    expect(sliceVolume(pts, 200, 400)).toEqual([pt(0, -10), pt(100, -20), pt(200, -20)]);
    expect(sliceVolume([], 0, 10)).toEqual([]);
  });

  it("adds and moves points", () => {
    const a = addPoint(pts, 200, -3);
    expect(a.index).toBe(1);
    expect(a.points.map((p) => p.position)).toEqual([100, 200, 300]);
    const m = movePoint(a.points, 1, 1_000, 99, 2_000);
    expect(m[1]).toEqual(pt(299, 12));
  });
});

describe("pan envelope", () => {
  it("draws full left at the top, center in the middle, full right at the bottom", () => {
    expect(valueToY("pan", -100, 10, 100)).toBe(10);
    expect(valueToY("pan", 0, 10, 100)).toBe(60);
    expect(valueToY("pan", 100, 10, 100)).toBe(110);
    expect(valueToY("pan", 400, 10, 100)).toBe(110);
  });

  it("reads y back to whole pan steps", () => {
    expect(yToValue("pan", 60, 10, 100)).toBe(0);
    expect(yToValue("pan", 35.4, 10, 100)).toBe(-49);
    expect(yToValue("pan", 999, 10, 100)).toBe(100);
  });

  it("volume mapping matches the dB helpers", () => {
    expect(valueToY("volume", -6, 0, 72)).toBeCloseTo(dbToY(-6, 0, 72));
    expect(yToValue("volume", 30, 0, 72)).toBeCloseTo(yToDb(30, 0, 72));
  });

  it("clamps moved points to the pan range", () => {
    const next = movePoint([pt(0, 0)], 0, 0, -300, 1000, "pan");
    expect(next[0]!.value).toBe(-100);
  });

  it("labels values", () => {
    expect(formatValue("pan", 0)).toBe("C");
    expect(formatValue("pan", -40)).toBe("L 40");
    expect(formatValue("pan", 25.4)).toBe("R 25");
    expect(formatValue("volume", 3)).toBe("+3.0 dB");
    expect(formatValue("volume", -6)).toBe("-6.0 dB");
  });
});

describe("effect envelopes", () => {
  const info = {
    id: "altaudio.phase_rotate",
    name: "Phase Rotate",
    category: "Filter and EQ",
    description: "",
    params: [
      { key: "angle", name: "Angle", unit: "°", min: -180, max: 180, default: 0, step: 1, log: false, toggle: false },
      { key: "printlen", name: "Hidden", unit: "", min: 0, max: 1, default: 0, step: 1, log: false, toggle: false, hidden: true },
    ],
  };
  const clip = {
    id: "c1",
    effects: { enabled: true, input_gain_db: 0, output_gain_db: 0, mix: 1, slots: [{ id: "s1", effect: info.id, enabled: true, params: { angle: 45 }, preset: null }] },
    envelopes: [{ id: "e", target: { type: "effect_param", slot_id: "s1", param: "angle" }, enabled: true, points: [{ position: 0, value: 90, curve: "linear" }] }],
  } as unknown as ClipDoc;

  it("lists the settings that can move and finds the clip's lane", () => {
    setFxCatalog([info]);
    expect(fxChoices(clip).map((c) => c.label)).toEqual(["Phase Rotate: Angle"]);
    const lane = fxLaneOf(clip, {});
    expect(lane).toEqual({ slot: "s1", param: "angle" });
    expect(rangeOf("fx", clip, lane)).toEqual({ top: 180, bottom: -180, step: 1 });
    expect(pointsOf(clip, "fx", lane).map((p) => p.value)).toEqual([90]);
    expect(restValue("fx", clip, lane)).toBe(45);
    expect(envChange(clip, "fx", [], lane)).toEqual({ id: "c1", effect_points: [{ slot_id: "s1", param: "angle", points: [] }] });
    expect(valueToY("fx", 180, 0, 100, rangeOf("fx", clip, lane))).toBe(0);
    expect(formatValue("fx", 12.345, "°")).toBe("12.3 °");
  });
});

describe("snapping", () => {
  it("pulls volume to 0 dB and pan to center when close", () => {
    expect(snapValue("volume", 0.5)).toBe(0);
    expect(snapValue("volume", -1.2)).toBe(-1.2);
    expect(snapValue("pan", -3)).toBe(0);
    expect(snapValue("pan", 10)).toBe(10);
    expect(snapValue("fx", 0.1)).toBe(0.1);
  });
});
