// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { clipLength, type ClipDoc, type MediaRefDoc } from "../../stores/session";
import { naturalLength, parseSeconds, percent, stretchTo, unstretch } from "./stretch";

const fade = (length = 0) => ({ length, shape: "linear", curve: 0 });
const m: MediaRefDoc = { id: "m", path: "a.wav", relative_path: null, sample_rate: 48_000, channels: 1, length: 4_800_000 } as MediaRefDoc;
const clip = (extra: Partial<ClipDoc> = {}): ClipDoc =>
  ({ id: "a", name: "a", media_id: "m", start: 0, source_in: 0, source_out: 48_000 * 28, gain_db: 0, muted: false, locked: false, color: null, fade_in: fade(4_800), fade_out: fade(), ...extra }) as ClipDoc;

describe("time stretch", () => {
  it("fits a 28 second read into a 30 second slot, keeping pitch", () => {
    const r = stretchTo(clip(), m, 48_000, 30 * 48_000, true)!;
    expect(r.ratio).toBeCloseTo(30 / 28, 5);
    expect(r.change.stretch).toEqual({ ratio: r.ratio, keep_pitch: true });
    const after = { ...clip(), stretch: r.change.stretch! };
    expect(clipLength(after, m, 48_000)).toBe(30 * 48_000);
    // The fade in grows with the clip so it covers the same audio.
    expect(r.change.fade_in?.length).toBe(Math.round(4_800 * r.ratio));
  });
  it("moves envelope points with the stretch", () => {
    const c = clip({ envelopes: [{ id: "e", target: { type: "volume" }, enabled: true, points: [{ position: 48_000, value: -6, curve: "linear" }] }] });
    const r = stretchTo(c, m, 48_000, 56 * 48_000, false)!;
    expect(r.change.volume![0]!.position).toBe(96_000);
  });
  it("restretches from the current stretch, and back to natural", () => {
    const c = clip({ stretch: { ratio: 2, keep_pitch: true } });
    expect(clipLength(c, m, 48_000)).toBe(56 * 48_000);
    expect(naturalLength(c, m, 48_000)).toBe(28 * 48_000);
    const back = unstretch(c, m, 48_000)!;
    expect(back.unstretch).toBe(true);
  });
  it("limits extreme ratios and refuses looped clips", () => {
    expect(stretchTo(clip(), m, 48_000, 48_000, true)!.limited).toBe(true);
    expect(stretchTo(clip({ looped: { length: 96_000, phase: 0 } }), m, 48_000, 96_000, true)).toBeNull();
  });
  it("reads times and shows percentages", () => {
    expect(parseSeconds("30")).toBe(30);
    expect(parseSeconds("1:05.5")).toBe(65.5);
    expect(parseSeconds("0:01:05")).toBe(65);
    expect(parseSeconds("abc")).toBeNull();
    expect(percent(1.125)).toBe("112.5%");
  });
});
