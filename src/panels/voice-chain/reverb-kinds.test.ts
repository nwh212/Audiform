// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { fromImpulse, toImpulse } from "./reverb-kinds";

describe("switching between made and recorded reverb", () => {
  it("carries the levels and shared settings across", () => {
    const ir = toImpulse({ dry: 100, wet: 50, predelay: 20, width: 80, size: 40 });
    expect(ir.dry).toBe(0);
    expect(ir.wet).toBeCloseTo(-6.02, 1);
    expect(ir.predelay).toBe(20);
    expect(ir.size).toBeUndefined();
    const back = fromImpulse(ir, 1);
    expect(back.kind).toBe(1);
    expect(back.wet).toBeCloseTo(50, 0);
    expect(fromImpulse({ wet: -48 }, 0).wet).toBe(0);
  });
});
