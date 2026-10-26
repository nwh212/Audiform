// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { binsOf, defaultColor, initials, missingStarterBins, PALETTE, STARTER_BINS } from "./bins";

describe("bins", () => {
  const lib = {
    collections: [
      { id: "a", name: "ads", items: ["x"], color: null },
      { id: "b", name: "Mine", items: ["x", "y"], color: null },
    ],
  };

  it("adds only the starter bins that are missing", () => {
    const m = missingStarterBins(lib);
    expect(m.length).toBe(STARTER_BINS.length - 1);
    expect(m.some((b) => b.name === "Ads")).toBe(false);
  });

  it("finds an item's bins", () => {
    expect(binsOf(lib, "x").map((c) => c.id)).toEqual(["a", "b"]);
    expect(binsOf(lib, "z")).toEqual([]);
  });

  it("makes show badges", () => {
    expect(initials("Morning Show")).toBe("MS");
    expect(initials("SHOWCAST")).toBe("SH");
    expect(initials("  ")).toBe("?");
    expect(PALETTE).toContain(defaultColor("Show"));
    expect(defaultColor("Show")).toBe(defaultColor("Show"));
  });
});
