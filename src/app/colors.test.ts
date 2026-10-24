// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { hexToRgb, ramp, rgbToHex, rgbToHsl, spectrumLut } from "./colors";

describe("color ramps", () => {
  it("reads and writes hex", () => {
    expect(hexToRgb("#ff8000")).toEqual([255, 128, 0]);
    expect(hexToRgb("nope")).toBeNull();
    expect(rgbToHex([255, 128, 0])).toBe("#ff8000");
  });

  it("goes up the color wheel from low to high", () => {
    const red = hexToRgb("#ff0000")!;
    const magenta = hexToRgb("#ff00ff")!;
    expect(rgbToHex(ramp(red, magenta, 0))).toBe("#ff0000");
    expect(rgbToHex(ramp(red, magenta, 1))).toBe("#ff00ff");
    // Halfway from 0° to 300° is 150°: green-cyan, not a dull mix of the two ends.
    expect(Math.round(rgbToHsl(ramp(red, magenta, 0.5))[0])).toBe(150);
  });

  it("builds a spectrogram palette from silence to the loud color", () => {
    const lut = spectrumLut("#0000ff", "#ffff00");
    expect([...lut.slice(0, 3)]).toEqual([8, 8, 13]);
    expect([...lut.slice(255 * 3)]).toEqual([255, 255, 0]);
    // Bad input falls back to the defaults rather than failing.
    expect(spectrumLut("x", "y").length).toBe(768);
  });
});
