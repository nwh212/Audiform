// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { anchorStart, clipsFrom, clipsUpTo, nextClipId } from "./clip-select";

const clips = [
  { id: "a", start: 0 },
  { id: "b", start: 100 },
  { id: "c", start: 200 },
];

describe("clip selection rules", () => {
  it("counts from the earliest chosen clip, or the playhead", () => {
    expect(anchorStart(clips, new Set(["c", "b"]), 5)).toBe(100);
    expect(anchorStart(clips, new Set(), 150)).toBe(150);
  });

  it("selects clips from or up to a position", () => {
    expect(clipsFrom(clips, 100)).toEqual(["b", "c"]);
    expect(clipsUpTo(clips, 100)).toEqual(["a", "b"]);
    expect(clipsFrom(clips, 201)).toEqual([]);
  });

  it("finds the next clip", () => {
    expect(nextClipId(clips, new Set(["a"]), 0)).toBe("b");
    expect(nextClipId(clips, new Set(["a", "b"]), 0)).toBe("c");
    expect(nextClipId(clips, new Set(["c"]), 0)).toBeNull();
    expect(nextClipId(clips, new Set(), 50)).toBe("b");
    expect(nextClipId(clips, new Set(), 100)).toBe("b");
  });
});
