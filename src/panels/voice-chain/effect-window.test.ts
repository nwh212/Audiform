// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { clampBox } from "./effect-window";

describe("clampBox", () => {
  it("keeps the window on screen", () => {
    expect(clampBox({ x: -50, y: -10 }, 400, 300, 1000, 800)).toEqual({ x: 0, y: 0 });
    expect(clampBox({ x: 900, y: 790 }, 400, 300, 1000, 800)).toEqual({ x: 600, y: 768 });
    expect(clampBox({ x: 10.4, y: 20.6 }, 400, 300, 1000, 800)).toEqual({ x: 10, y: 21 });
  });
  it("copes with a window wider than the screen", () => {
    expect(clampBox({ x: 50, y: 50 }, 1200, 300, 1000, 800)).toEqual({ x: 0, y: 50 });
  });
});
