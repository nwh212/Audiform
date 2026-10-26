// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { SessionDoc } from "../../stores/session";
import { alignOps, alignPair, describeAlign } from "./align";

const doc = {
  sample_rate: 48_000,
  tracks: [
    { id: "t1", polarity_inverted: false, clips: [{ id: "a", start: 1000, locked: false }] },
    { id: "t2", polarity_inverted: false, clips: [{ id: "b", start: 1000, locked: false }, { id: "c", start: 9000, locked: false }] },
  ],
} as unknown as SessionDoc;

describe("phase align", () => {
  it("uses the upper clip as the reference and needs two tracks", () => {
    expect(alignPair(doc, ["b", "a"])).toEqual({ reference: "a", other: "b", trackId: "t2" });
    expect(alignPair(doc, ["b", "c"])).toBeNull();
    expect(alignPair(doc, ["a"])).toBeNull();
  });

  it("moves the clip and flips polarity in one edit", () => {
    expect(alignOps(doc, "b", "t2", { lag: 120, score: 0.9, inverted: false })).toEqual({ op: "set_clips", changes: [{ id: "b", start: 880 }] });
    expect(alignOps(doc, "b", "t2", { lag: -50, score: 0.9, inverted: true })).toEqual({
      op: "batch",
      ops: [
        { op: "set_clips", changes: [{ id: "b", start: 1050 }] },
        { op: "set_track", id: "t2", changes: { polarity_inverted: true } },
      ],
    });
    expect(alignOps(doc, "b", "t2", { lag: 0, score: 1, inverted: false })).toBeNull();
    expect(describeAlign({ lag: 120, score: 0.9, inverted: false }, 48_000, "Guest", "Guest Mic")).toBe("Moved Guest 2.50 ms earlier (90% match).");
  });
});
