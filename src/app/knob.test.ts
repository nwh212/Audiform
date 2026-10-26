// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { DRAG_PIXELS, dragValue, keyValue, pointerAngle } from "./knob";

describe("dial maths", () => {
  it("drags up to raise and down to lower, within the range", () => {
    expect(dragValue(0.5, DRAG_PIXELS / 4)).toBeCloseTo(0.75);
    expect(dragValue(0.5, -DRAG_PIXELS / 4)).toBeCloseTo(0.25);
    expect(dragValue(0.9, 1000)).toBe(1);
    expect(dragValue(0.1, -1000)).toBe(0);
  });
  it("moves in fine steps with Ctrl and coarse steps with Shift", () => {
    const plain = dragValue(0.5, 16) - 0.5;
    expect(dragValue(0.5, 16, { ctrl: true }) - 0.5).toBeLessThan(plain);
    expect(dragValue(0.5, 16, { shift: true }) - 0.5).toBeGreaterThan(plain);
  });
  it("steps with the keys and goes home on Home", () => {
    expect(keyValue(0.5, "ArrowUp", 0.7)).toBeCloseTo(0.51);
    expect(keyValue(0.5, "PageDown", 0.7)).toBeCloseTo(0.4);
    expect(keyValue(0.5, "Home", 0.7)).toBe(0.7);
    expect(keyValue(0.5, "a", 0.7)).toBeNull();
    expect(keyValue(1, "ArrowUp", 0.7)).toBe(1);
  });
  it("points left at the bottom of the range and right at the top", () => {
    expect(pointerAngle(0)).toBe(-135);
    expect(pointerAngle(0.5)).toBe(0);
    expect(pointerAngle(1)).toBe(135);
  });
});
