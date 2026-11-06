// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform.

import { describe, expect, it } from "vitest";
import { PITCH_BEND, eraseControls, movesOf, paintControl, shapeValue, valueText } from "./midi-controls";

describe("controller lane", () => {
  it("pedals are up or down", () => {
    expect(shapeValue(64, 0.3)).toBe(0);
    expect(shapeValue(64, 0.7)).toBe(1);
    expect(valueText(64, 1)).toBe("down");
    expect(shapeValue(1, 0.5)).toBeCloseTo(64 / 127);
    expect(valueText(PITCH_BEND, 0.5)).toBe("0");
  });

  it("drawing replaces moves in the span and keeps other controllers", () => {
    let c = paintControl([], 1, 0, 0, 0.2);
    c = paintControl(c, 1, 1, 1, 0.4);
    c = paintControl(c, 7, 1, 1, 0.9);
    c = paintControl(c, 1, 0.5, 2, 0.6);
    expect(movesOf(c, 1).map((m) => [m.at, m.value])).toEqual([
      [0, 0.2],
      [2, 0.6],
    ]);
    expect(movesOf(c, 7)).toHaveLength(1);
  });

  it("a pedal keeps only changes", () => {
    let c = paintControl([], 64, 0, 0, 1);
    c = paintControl(c, 64, 1, 1, 1);
    c = paintControl(c, 64, 2, 2, 0);
    expect(movesOf(c, 64).map((m) => m.value)).toEqual([1, 0]);
  });

  it("erases a span", () => {
    let c = paintControl([], 1, 0, 0, 0.2);
    c = paintControl(c, 1, 3, 3, 0.4);
    expect(movesOf(eraseControls(c, 1, 2, 4), 1)).toHaveLength(1);
  });
});
