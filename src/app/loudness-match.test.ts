// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { targetById, type LoudnessReport } from "./loudness";
import { matchChanges, matchItems, matchRow } from "./loudness-match";
import type { SessionDoc } from "../stores/session";

const rep = (i: number, tp: number, mono = i - 3): LoudnessReport => ({ integrated: i, integratedMono: mono, range: 5, truePeak: tp, samplePeak: tp, seconds: 60, cancelled: false });
const doc = {
  tracks: [
    { id: "h", name: "Host", muted: false, clips: [{ id: "a", name: "a", gain_db: 0 }, { id: "b", name: "b", gain_db: -2 }] },
    { id: "g", name: "Guest", muted: false, clips: [{ id: "c", name: "c", gain_db: 20 }] },
    { id: "e", name: "Empty", muted: false, clips: [] },
  ],
} as unknown as SessionDoc;

describe("loudness match", () => {
  it("works out the gain to the target", () => {
    const r = matchRow({ key: "h", label: "Host", clipIds: ["a"] }, rep(-22.4, -6), targetById("podcast-stereo"));
    expect(r.change).toBe(6.4);
    expect(r.peakAfter).toBe(0.4);
    expect(r.peakWarning).toBe(true);
  });
  it("uses the mono reading for mono targets", () => {
    const r = matchRow({ key: "h", label: "Host", clipIds: ["a"] }, rep(-16, -10, -20), targetById("podcast-mono"));
    expect(r.change).toBe(1);
  });
  it("skips silence", () => {
    const r = matchRow({ key: "h", label: "Host", clipIds: ["a"] }, rep(-Infinity, -Infinity), targetById("podcast-stereo"));
    expect(r.measurable).toBe(false);
    expect(matchChanges(doc, [r])).toEqual([]);
  });
  it("moves every clip on a track together and keeps gain in range", () => {
    const t = targetById("podcast-stereo");
    const host = matchRow({ key: "h", label: "Host", clipIds: ["a", "b"] }, rep(-19, -9), t);
    const guest = matchRow({ key: "g", label: "Guest", clipIds: ["c"] }, rep(-26, -20), t);
    expect(matchChanges(doc, [host, guest])).toEqual([
      { id: "a", gain_db: 3 },
      { id: "b", gain_db: 1 },
      { id: "c", gain_db: 24 },
    ]);
  });
  it("measures selected clips one by one, otherwise each track with clips", () => {
    expect(matchItems(doc, new Set(["c"])).items.map((i) => i.key)).toEqual(["c"]);
    const all = matchItems(doc, new Set());
    expect(all.mode).toBe("tracks");
    expect(all.items.map((i) => i.key)).toEqual(["h", "g"]);
  });
});
