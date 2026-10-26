// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.
import { describe, expect, it } from "vitest";
import { hasClipIn, showOnly, shownTracks } from "./visibility";

const t = (id: string, clips: number, hidden = false) => ({
  id,
  hidden,
  clips: Array.from({ length: clips }, (_, i) => ({ id: `${id}${i}` })),
});

describe("track visibility", () => {
  it("lists only shown tracks", () => {
    expect(shownTracks([t("a", 1), t("b", 0, true)]).map((x) => x.id)).toEqual([
      "a",
    ]);
  });
  it("reports only the tracks whose flag changes", () => {
    const tracks = [t("a", 1), t("b", 0), t("c", 2, true)];
    expect(showOnly(tracks, (x) => x.clips.length > 0)).toEqual([
      { id: "b", hidden: true },
      { id: "c", hidden: false },
    ]);
    expect(showOnly([t("a", 1)], () => true)).toEqual([]);
  });
  it("finds clips in a range or at a point", () => {
    const spans = [{ start: 100, length: 50 }];
    expect(hasClipIn(spans, 120, 130)).toBe(true);
    expect(hasClipIn(spans, 150, 200)).toBe(false);
    expect(hasClipIn(spans, 149, 149)).toBe(true);
    expect(hasClipIn(spans, 150, 150)).toBe(false);
  });
});
