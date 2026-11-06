// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { ClipDoc, SessionDoc, TrackDoc } from "../../stores/session";
import { TOP_LAYER } from "./clip-edit";
import { stackChanges } from "./clip-order";

const clip = (id: string, locked = false): ClipDoc => ({ id, name: id, media_id: "m", start: 0, source_in: 0, source_out: 100, gain_db: 0, muted: false, locked, color: null } as unknown as ClipDoc);
const doc = (...tracks: ClipDoc[][]): SessionDoc => ({ tracks: tracks.map((clips, i) => ({ id: `t${i}`, clips }) as unknown as TrackDoc) }) as unknown as SessionDoc;

describe("stackChanges", () => {
  it("brings clips to the front in their own order", () => {
    const d = doc([clip("a"), clip("b"), clip("c")]);
    expect(stackChanges(d, new Set(["a", "b"]), "front")).toEqual([
      { id: "a", layer: TOP_LAYER },
      { id: "b", layer: TOP_LAYER },
    ]);
  });

  it("sends clips to the back, last one first, so their order holds", () => {
    const d = doc([clip("a"), clip("b"), clip("c")]);
    expect(stackChanges(d, new Set(["b", "c"]), "back")).toEqual([
      { id: "c", layer: 0 },
      { id: "b", layer: 0 },
    ]);
  });

  it("does nothing for clips already there, locked clips, or other tracks", () => {
    const d = doc([clip("a"), clip("b")], [clip("x", true), clip("y")]);
    expect(stackChanges(d, new Set(["b"]), "front")).toEqual([]);
    expect(stackChanges(d, new Set(["a"]), "back")).toEqual([]);
    expect(stackChanges(d, new Set(["x"]), "front")).toEqual([]);
    expect(stackChanges(d, new Set(["y"]), "back")).toEqual([{ id: "y", layer: 0 }]);
  });
});
