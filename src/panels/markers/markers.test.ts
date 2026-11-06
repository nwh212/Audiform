// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { deleteLabel, markerAt, markersBetween, newMarker, nextMarkerName, pickedMarkers } from "./markers";

describe("markers", () => {
  const ms = [newMarker("a", "Marker 1", 100), newMarker("b", "Marker 3", 140, 50, "chapter")];

  it("names new markers without clashing", () => {
    expect(nextMarkerName(ms)).toBe("Marker 4");
    expect(nextMarkerName([newMarker("x", "Marker 2", 0)])).toBe("Marker 3");
  });

  it("finds the flag under the pointer", () => {
    const xOf = (s: number) => s / 10;
    expect(markerAt(ms, 11, xOf)?.id).toBe("a");
    expect(markerAt(ms, 14, xOf)?.id).toBe("b");
    expect(markerAt(ms, 30, xOf)).toBeNull();
  });

  it("picks markers by id in time order and ignores ids that no longer exist", () => {
    const list = [newMarker("c", "Marker 3", 300), newMarker("a", "Marker 1", 100), newMarker("b", "Marker 2", 200)];
    expect(pickedMarkers(list, new Set(["c", "a", "gone"])).map((m) => m.id)).toEqual(["a", "c"]);
    expect(pickedMarkers(list, new Set())).toEqual([]);
  });

  it("selects a run of markers between two clicks, whichever way round", () => {
    const list = [newMarker("c", "Marker 3", 300), newMarker("a", "Marker 1", 100), newMarker("b", "Marker 2", 200), newMarker("d", "Marker 4", 400)];
    expect(markersBetween(list, "a", "c")).toEqual(["a", "b", "c"]);
    expect(markersBetween(list, "d", "b")).toEqual(["b", "c", "d"]);
    expect(markersBetween(list, "a", "a")).toEqual(["a"]);
    expect(markersBetween(list, "a", "gone")).toEqual([]);
  });

  it("labels the undo step for one or several markers", () => {
    expect(deleteLabel(1)).toBe("Delete Marker");
    expect(deleteLabel(3)).toBe("Delete 3 Markers");
  });
});
