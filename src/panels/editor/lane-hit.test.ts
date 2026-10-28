// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { ClipDoc, TrackDoc } from "../../stores/session";
import { hiddenEdges, zoneAt } from "./lane-hit";

const fade = { length: 0, shape: "linear", curve: 0 };
const clip = (id: string, start: number, length: number): ClipDoc =>
  ({ id, name: id, media_id: "m", start, source_in: 0, source_out: length, gain_db: 0, muted: false, locked: false, color: null, fade_in: fade, fade_out: fade }) as unknown as ClipDoc;
const g = { view: { spp: 1, scroll: 0 }, media: new Map(), rate: 48000 };
const ed = { stretchHandles: false, fxLanes: {}, showVolume: true, showPan: false, showFx: false };

describe("clip edges under another clip", () => {
  // "a" runs 0..300 under "b", which starts at 200: a's end at 300 is hidden.
  const track = { id: "t", height: 80, clips: [clip("a", 0, 300), clip("b", 200, 300)] } as unknown as TrackDoc;

  it("finds the hidden end and how far the top clip covers it", () => {
    expect(hiddenEdges(g, track)).toEqual([{ clip: track.clips[0], side: "end", x: 300, to: 200 }]);
  });

  it("lets the hidden end be grabbed below the fade handles, and shows a trim there", () => {
    const z = zoneAt(g, ed, track, 301, 60);
    expect(z?.kind).toBe("trim-right");
    expect(z?.clip.id).toBe("a");
  });

  it("puts a clip's own edges before its volume line", () => {
    expect(zoneAt(g, ed, track, 497, 40)?.kind).toBe("trim-right");
  });
});
