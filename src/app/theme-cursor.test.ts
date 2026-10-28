// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { beforeAll, describe, expect, it } from "vitest";
import { keywordOf, themedFor, type ResolvedPointers } from "./theme-cursor";
import { findTheme, resolvePointers } from "../themes/loader";

/** A shipped theme's pointers, read from its file like the app does. */
async function pointersOf(id: string): Promise<ResolvedPointers | null> {
  const theme = await findTheme(id, []);
  return theme ? resolvePointers(theme) : null;
}

let prism: ResolvedPointers | null | undefined;
let aqua: ResolvedPointers | null | undefined;
let meadow: ResolvedPointers | null | undefined;
let phosphor: ResolvedPointers | null | undefined;
let mallpastel: ResolvedPointers | null | undefined;
let mallnight: ResolvedPointers | null | undefined;
let analog: ResolvedPointers | null | undefined;

beforeAll(async () => {
  [prism, aqua, meadow, phosphor, mallpastel, mallnight, analog] = await Promise.all(
    ["prism", "aqua", "meadow", "phosphor", "mallpastel", "mallnight", "analog"].map(pointersOf),
  );
});

describe("theme pointers", () => {
  it("reads the standard pointer from a cursor value", () => {
    expect(keywordOf("ew-resize")).toBe("ew-resize");
    expect(keywordOf('url("a.svg") 10 10, ns-resize')).toBe("ns-resize");
  });

  it("swaps text, resize and grab pointers, keeping the standard one as the fallback", () => {
    expect(themedFor("text", prism ?? null)).toMatch(/^url\(.+\) 10 10, text$/);
    expect(themedFor("col-resize", aqua ?? null)).toMatch(/, col-resize$/);
    expect(themedFor("grabbing", meadow ?? null)).toMatch(/, grabbing$/);
    expect(themedFor("nwse-resize", phosphor ?? null)).toMatch(/^url\(.+\) 9 9, nwse-resize$/);
  });

  it("gives each pixel-art pointer its own hot spot", () => {
    expect(themedFor("text", phosphor ?? null)).toMatch(/\) 7 10, text$/);
    expect(themedFor("ew-resize", phosphor ?? null)).toMatch(/\) 11 5, ew-resize$/);
  });

  it("draws a busy pointer only for themes that have one", () => {
    expect(themedFor("progress", mallnight ?? null)).toMatch(/^url\(.+\) 8 8, progress$/);
    expect(themedFor("wait", mallpastel ?? null)).toMatch(/, wait$/);
    expect(themedFor("text", mallpastel ?? null)).toMatch(/\) 8 8, text$/);
    expect(themedFor("wait", prism ?? null)).toBeNull();
  });

  it("leaves other pointers and other themes alone", () => {
    expect(themedFor("default", prism ?? null)).toBeNull();
    expect(themedFor("crosshair", phosphor ?? null)).toBeNull();
    expect(themedFor("text", null)).toBeNull();
    expect(analog).toBeNull();
  });
});
