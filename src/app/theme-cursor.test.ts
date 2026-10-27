// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { keywordOf, themedFor } from "./theme-cursor";

describe("theme pointers", () => {
  it("reads the standard pointer from a cursor value", () => {
    expect(keywordOf("ew-resize")).toBe("ew-resize");
    expect(keywordOf('url("a.svg") 10 10, ns-resize')).toBe("ns-resize");
  });

  it("swaps text, resize and grab pointers, keeping the standard one as the fallback", () => {
    expect(themedFor("text", "prism")).toMatch(/^url\(.+\) 10 10, text$/);
    expect(themedFor("col-resize", "aqua")).toMatch(/, col-resize$/);
    expect(themedFor("grabbing", "meadow")).toMatch(/, grabbing$/);
    expect(themedFor("nwse-resize", "phosphor")).toMatch(/^url\(.+\) 9 9, nwse-resize$/);
  });

  it("gives each pixel-art pointer its own hot spot", () => {
    expect(themedFor("text", "phosphor")).toMatch(/\) 7 10, text$/);
    expect(themedFor("ew-resize", "phosphor")).toMatch(/\) 11 5, ew-resize$/);
  });

  it("draws a busy pointer only for themes that have one", () => {
    expect(themedFor("progress", "mallnight")).toMatch(/^url\(.+\) 12 12, progress$/);
    expect(themedFor("wait", "mallpastel")).toMatch(/, wait$/);
    expect(themedFor("text", "mallpastel")).toMatch(/\) 12 12, text$/);
    expect(themedFor("wait", "prism")).toBeNull();
  });

  it("leaves other pointers and other themes alone", () => {
    expect(themedFor("default", "prism")).toBeNull();
    expect(themedFor("crosshair", "phosphor")).toBeNull();
    expect(themedFor("text", "dark")).toBeNull();
  });
});
