// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { dragGain, gainBox, gainText } from "./clip-gain";

describe("clip level handle", () => {
  it("raises the level dragging up or right and lowers it down or left", () => {
    expect(dragGain(0, 30, 0, false)).toBe(3);
    expect(dragGain(0, 0, -30, false)).toBe(3);
    expect(dragGain(0, -20, 40, false)).toBe(-6);
    expect(dragGain(0, 50, 0, true)).toBe(1);
    expect(dragGain(20, 200, 0, false)).toBe(24);
  });

  it("sits in the bottom-left corner of clips big enough to hold it", () => {
    expect(gainBox(100, 300, 90, 20)).toEqual({ x: 104, y: 74, w: 14, h: 12 });
    expect(gainBox(100, 120, 90, 20)).toBeNull();
    expect(gainText(0)).toBe("");
    expect(gainText(-3)).toBe("-3.0 dB");
  });
});
