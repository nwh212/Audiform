// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { CORNER_REACH, handlesAt, type Box } from "./corner-drag";

// A vertical handle at x=300 from y=0 to 400, and two horizontal ones that
// meet it: one to its left (y=200), one to its right (y=210, a little off).
const vertical: Box = { left: 298, top: 0, right: 302, bottom: 400 };
const leftHorizontal: Box = { left: 0, top: 198, right: 298, bottom: 202 };
const rightHorizontal: Box = { left: 302, top: 208, right: 600, bottom: 212 };
const far: Box = { left: 0, top: 380, right: 298, bottom: 384 };

describe("four-corner resizing", () => {
  it("takes every handle that meets at the grab point, even when roughly aligned", () => {
    const boxes = [vertical, leftHorizontal, rightHorizontal, far];
    // Grabbing the vertical handle at the cross: both horizontals come along.
    expect(handlesAt(300, 205, boxes)).toEqual([0, 1, 2]);
    // Far from any other handle, only the one grabbed is found.
    expect(handlesAt(300, 40, boxes)).toEqual([0]);
  });

  it("does not take handles farther than the reach", () => {
    const off: Box = { left: 0, top: 198 + CORNER_REACH + 20, right: 298, bottom: 202 + CORNER_REACH + 20 };
    expect(handlesAt(300, 200, [vertical, off])).toEqual([0]);
  });
});
