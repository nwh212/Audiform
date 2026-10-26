// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { glassFor, keywordOf } from "./glass-cursor";

describe("glass pointers", () => {
  it("reads the standard pointer from a cursor value", () => {
    expect(keywordOf("ew-resize")).toBe("ew-resize");
    expect(keywordOf('url("a.svg") 10 10, ns-resize')).toBe("ns-resize");
  });

  it("swaps text and resize pointers, keeping the standard one as the fallback", () => {
    expect(glassFor("text")).toMatch(/^url\(.+\) 10 10, text$/);
    expect(glassFor("col-resize")).toMatch(/, col-resize$/);
    expect(glassFor("nwse-resize")).toMatch(/, nwse-resize$/);
  });

  it("leaves other pointers alone", () => {
    expect(glassFor("default")).toBeNull();
    expect(glassFor("grab")).toBeNull();
    expect(glassFor("crosshair")).toBeNull();
  });
});
