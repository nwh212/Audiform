// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { editSelection, parseTime, viewFor, viewRange } from "./time-info";

const view = { samples_per_pixel: 100, scroll: 1000, scroll_y: 0, playhead: 0, selection: null };

describe("Time Info", () => {
  it("reads typed times in the display format", () => {
    expect(parseTime("1:05.5", 1000, "time", 30)).toBe(65_500);
    expect(parseTime("0", 1000, "time", 30)).toBe(0);
    expect(parseTime("0:01:00", 1000, "time", 30)).toBe(60_000);
    expect(parseTime("4800", 48_000, "samples", 30)).toBe(4800);
    expect(parseTime("00:00:01:15", 1000, "timecode", 30)).toBe(1500);
    expect(parseTime("soon", 1000, "time", 30)).toBeNull();
  });

  it("shows and sets the visible range", () => {
    expect(viewRange(view, 500)).toEqual({ start: 1000, end: 51_000 });
    const v = viewFor(view, 2000, 12_000, 500);
    expect([v.scroll, v.samples_per_pixel]).toEqual([2000, 20]);
  });

  it("keeps an edited selection in order", () => {
    const sel = { start: 100, end: 200 };
    expect(editSelection(sel, "start", 300)).toEqual({ start: 200, end: 300 });
    expect(editSelection(sel, "duration", 50)).toEqual({ start: 100, end: 150 });
    expect(editSelection(sel, "end", -5)).toEqual({ start: 0, end: 100 });
  });
});
