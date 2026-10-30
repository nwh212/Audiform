// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { automatedPos, laneAt } from "./space";
import { DEFAULT_SPACE_POS, type EnvelopeDoc } from "../../stores/session";

describe("spots follow their automation", () => {
  const pts = [
    { position: 100, value: -1, curve: "linear" },
    { position: 200, value: 1, curve: "hold" },
    { position: 300, value: 0, curve: "linear" },
  ];
  it("reads a lane at a time", () => {
    expect(laneAt([], 50)).toBeNull();
    expect(laneAt(pts, 0)).toBe(-1);
    expect(laneAt(pts, 150)).toBe(0);
    expect(laneAt(pts, 250)).toBe(1);
    expect(laneAt(pts, 400)).toBe(0);
  });
  it("puts enabled space lanes in place, leaving the rest", () => {
    const env = (param: string, enabled = true): EnvelopeDoc => ({ id: param, target: { type: "space", param: param as "x" }, enabled, points: pts as EnvelopeDoc["points"] });
    const p = automatedPos({ ...DEFAULT_SPACE_POS, y: 0.5 }, [env("x"), env("z", false)], 150);
    expect(p.x).toBe(0);
    expect(p.y).toBe(0.5);
    expect(p.z).toBe(DEFAULT_SPACE_POS.z);
  });
});
