// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe as group, expect, it } from "vitest";
import { describe, resolve, sanitizeRoutingPresets, STEREO } from "./routing";

const BED = ["L", "R", "C", "LFE", "Lrs", "Rrs", "Lss", "Rss", "Ltf", "Rtf", "Ltr", "Rtr"];

group("effect routing", () => {
  it("maps by speaker name like the core", () => {
    const fx = ["L", "R", "C", "LFE", "Ls", "Rs"];
    expect(resolve(null, fx, fx, BED).inputs).toEqual([0, 1, 2, 3, 6, 7]);
    expect(describe(resolve(null, STEREO, STEREO, STEREO), STEREO)).toBe("In: L, R | Out: L, R");
  });
  it("uses the slot's own routing and keeps it in range", () => {
    expect(resolve({ inputs: [1, 0], outputs: [9, null] }, STEREO, STEREO, STEREO)).toEqual({ inputs: [1, 0], outputs: [null, null] });
  });
  it("reads presets safely", () => {
    expect(sanitizeRoutingPresets([{ name: "Swap", routing: { inputs: [1, 0], outputs: [1, "x"] } }, { name: 2 }])).toEqual([{ name: "Swap", routing: { wide: false, inputs: [1, 0], outputs: [1, null] } }]);
  });
});
