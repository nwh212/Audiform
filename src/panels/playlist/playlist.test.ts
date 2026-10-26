// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { newMarker } from "../markers/markers";
import type { SessionDoc } from "../../stores/session";
import { available, exportNames, nextStart, playlistItems, withAdded, withMoved, withRemoved } from "./playlist";

const doc = (playlist: string[]) =>
  ({
    markers: [newMarker("a", "Cold open", 0, 1000), newMarker("b", "Interview", 5000, 3000), newMarker("p", "Point", 2000), newMarker("c", "Outro", 9000, 500)],
    playlist,
  }) as unknown as SessionDoc;

describe("playlist", () => {
  it("lists range markers in playlist order and skips stale or point markers", () => {
    expect(playlistItems(doc(["c", "gone", "p", "a"])).map((m) => m.id)).toEqual(["c", "a"]);
    expect(available(doc(["c"])).map((m) => m.id)).toEqual(["a", "b"]);
  });
  it("adds, removes and moves", () => {
    expect(withAdded(doc(["a"]), ["b", "a"])).toEqual(["a", "b"]);
    expect(withRemoved(doc(["a", "b"]), "a")).toEqual(["b"]);
    expect(withMoved(doc(["a", "b", "c"]), "c", -1)).toEqual(["a", "c", "b"]);
    expect(withMoved(doc(["a", "b"]), "a", -1)).toEqual(["a", "b"]);
  });
  it("plays on to the next item, then stops", () => {
    const items = playlistItems(doc(["b", "a"]));
    expect(nextStart(items, 0)).toEqual({ index: 1, at: 0 });
    expect(nextStart(items, 1)).toBeNull();
  });
  it("numbers export names and keeps them unique and safe", () => {
    const m = [newMarker("1", "Ad: Sponsor/Read", 0, 1), newMarker("2", "Ad: Sponsor/Read", 0, 1)];
    expect(exportNames(m)).toEqual(["01 Ad Sponsor Read", "02 Ad Sponsor Read"]);
  });
});
