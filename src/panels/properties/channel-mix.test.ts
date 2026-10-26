// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { channelLabels, defaultMix, mixOf, presetFor, presets } from "./channel-mix";

describe("Source Channels", () => {
  it("names channels in the WAV order", () => {
    expect(channelLabels(6)).toEqual(["L", "R", "C", "LFE", "Ls", "Rs"]);
    expect(channelLabels(9)[8]).toBe("9");
  });

  it("folds surround down without the LFE", () => {
    const m = defaultMix(6).sends;
    expect(m[0]).toEqual({ left: 1, right: 0 });
    expect(m[3]).toEqual({ left: 0, right: 0 });
    expect(m[4]!.left).toBeCloseTo(Math.SQRT1_2);
  });

  it("offers each pair and each channel alone", () => {
    const ids = presets(12).map((p) => p.label);
    expect(ids).toContain("Lss and Rss only");
    expect(ids).toContain("Ltf and Rtf only");
    expect(ids).toContain("C alone, to both sides");
    expect(presets(2).map((p) => p.id)).toContain("swap");
  });

  it("recognizes a preset and fills a short mix", () => {
    expect(presetFor(defaultMix(6), 6)).toBe("default");
    expect(mixOf({ sends: [{ left: 1, right: 0 }] }, 3).sends).toHaveLength(3);
  });
});
