// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { placeTake, punchStopAfter, recordPlan, RECORD_DEFAULTS, sanitizeRecordOptions } from "./recording";

const R = 48_000;

describe("recording placement", () => {
  it("records from the playhead without a selection", () => {
    const p = recordPlan(96_000, null, RECORD_DEFAULTS, R);
    expect(p).toEqual({ at: 96_000, punch: null });
    expect(placeTake(p, { length: 48_000, sampleRate: R }, R, 0)).toEqual({ start: 96_000, sourceIn: 0, sourceOut: 48_000 });
  });
  it("trims the latency offset from the start of the take", () => {
    const p = recordPlan(0, null, RECORD_DEFAULTS, R);
    expect(placeTake(p, { length: 48_000, sampleRate: R }, R, 10)).toEqual({ start: 0, sourceIn: 480, sourceOut: 48_000 });
  });
  it("punches in with pre-roll and keeps only the selection", () => {
    const sel = { start: 10 * R, end: 14 * R };
    const p = recordPlan(0, sel, RECORD_DEFAULTS, R);
    expect(p.at).toBe(8 * R);
    expect(punchStopAfter(p, R)).toBe(6);
    // Take recorded at 44.1 kHz into a 48 kHz session.
    const placed = placeTake(p, { length: 44_100 * 7, sampleRate: 44_100 }, R, 0)!;
    expect(placed).toEqual({ start: 10 * R, sourceIn: 2 * 44_100, sourceOut: 6 * 44_100 });
  });
  it("pre-roll stops at the session start", () => {
    expect(recordPlan(0, { start: R, end: 2 * R }, RECORD_DEFAULTS, R).at).toBe(0);
  });
  it("can turn punch-in off", () => {
    expect(recordPlan(5, { start: R, end: 2 * R }, { ...RECORD_DEFAULTS, punch: false }, R)).toEqual({ at: 5, punch: null });
  });
  it("cleans saved options", () => {
    expect(sanitizeRecordOptions({ offsetMs: 5000, prerollSec: -1, punch: false })).toEqual({ offsetMs: 1000, punch: false, prerollSec: 0, replace: true });
  });
});
