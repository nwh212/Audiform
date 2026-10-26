// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { markerAt, newMarker, nextMarkerName } from "./markers";

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
});
