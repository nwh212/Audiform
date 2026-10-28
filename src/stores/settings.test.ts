// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { colorVars, DEFAULT_SETTINGS, meterPercent, sanitizeSettings, settings, zoomRatios } from "./settings";

describe("app settings", () => {
  it("keeps defaults for anything missing or wrong", () => {
    expect(sanitizeSettings(null)).toEqual(DEFAULT_SETTINGS);
    const s = sanitizeSettings({ zoomStep: 500, fadeShape: "wobbly", colors: { wave: "#ABCDEF", accent: "red" }, output: "  ", autosaveMinutes: 0, meterYellowDb: -10, meterRedDb: -20 });
    expect(s.zoomStep).toBe(100);
    expect(s.fadeShape).toBe("linear");
    expect(s.colors).toEqual({ wave: "#abcdef", playhead: "", accent: "", laneSelected: "", spectrumLow: "", spectrumHigh: "", phaseLow: "", phaseHigh: "" });
    expect(s.output).toBeNull();
    expect(s.autosaveMinutes).toBe(1);
    expect(s.meterRedDb).toBe(-9);
  });

  it("turns settings into zoom ratios, colors and meter marks", () => {
    expect(zoomRatios(25)).toEqual({ zoomIn: 0.8, zoomOut: 1.25 });
    expect(colorVars({ wave: "#112233", playhead: "", accent: "", laneSelected: "", spectrumLow: "", spectrumHigh: "", phaseLow: "", phaseHigh: "" })).toEqual({ "--wave": "#112233" });
    expect(meterPercent(-12)).toBeCloseTo(80);
    expect(meterPercent(-3)).toBeCloseTo(95);
    let seen = 0;
    const off = settings.subscribe((s) => (seen = s.zoomStep));
    settings.set({ ...DEFAULT_SETTINGS, zoomStep: 40 });
    expect(seen).toBe(40);
    off();
    settings.set(DEFAULT_SETTINGS);
  });
});

describe("theme setting", () => {
  it("keeps any theme id, so themes from the Themes folder survive", () => {
    expect(sanitizeSettings({ theme: "meadow" }).theme).toBe("meadow");
    expect(sanitizeSettings({ theme: "my-own-theme" }).theme).toBe("my-own-theme");
    expect(sanitizeSettings({ theme: "mallnight" }).theme).toBe("mallnight");
  });

  it("migrates the old names", () => {
    expect(sanitizeSettings({ theme: "sky" }).theme).toBe("meadow");
    expect(sanitizeSettings({ theme: "night" }).theme).toBe("moonlight");
  });

  it("falls back to dark for anything that is not an id", () => {
    expect(sanitizeSettings({}).theme).toBe("dark");
    expect(sanitizeSettings({ theme: 5 }).theme).toBe("dark");
    expect(sanitizeSettings({ theme: "Not An Id" }).theme).toBe("dark");
    expect(sanitizeSettings({ theme: "../x" }).theme).toBe("dark");
  });

  it("keeps unlocked secret theme ids as a clean list", () => {
    expect(sanitizeSettings({}).unlockedThemes).toEqual([]);
    expect(sanitizeSettings({ unlockedThemes: ["a-b", "a-b", "Bad Id", 3, "dark", "c"] }).unlockedThemes).toEqual(["a-b", "c"]);
    expect(sanitizeSettings({ unlockedThemes: "x" }).unlockedThemes).toEqual([]);
  });
});
