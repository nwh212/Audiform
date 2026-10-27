// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { ClipDoc, MediaRefDoc } from "../../stores/session";
import { gestureLabel, slipChange } from "./clip-drag";
import { ghostItems } from "./drop-ghost";

const fade = (length = 0) => ({ length, shape: "linear", curve: 0 });
const m: MediaRefDoc = { id: "m", path: "a.wav", relative_path: null, sample_rate: 48_000, channels: 1, length: 480_000 } as MediaRefDoc;
const clip = (extra: Partial<ClipDoc> = {}): ClipDoc =>
  ({ id: "a", name: "a", media_id: "m", start: 0, source_in: 48_000, source_out: 96_000, gain_db: 0, muted: false, locked: false, color: null, fade_in: fade(), fade_out: fade(), ...extra }) as ClipDoc;

describe("clip drags", () => {
  it("slips the audio inside the clip, within the file", () => {
    // Dragging right brings earlier audio in.
    expect(slipChange(clip(), m, 48_000, 24_000)).toEqual({ id: "a", source_in: 24_000, source_out: 72_000 });
    expect(slipChange(clip(), m, 48_000, 96_000)).toEqual({ id: "a", source_in: 0, source_out: 48_000 });
    expect(slipChange(clip(), m, 48_000, -10_000_000)).toEqual({ id: "a", source_in: 432_000, source_out: 480_000 });
  });
  it("turns a looped clip's phase instead", () => {
    const c = clip({ looped: { length: 96_000, phase: 0 } });
    expect(slipChange(c, m, 48_000, 12_000).looped).toEqual({ length: 96_000, phase: 36_000 });
  });
  it("labels the finished drag", () => {
    expect(gestureLabel("move", false, 3, [])).toBe("Move 3 Clips");
    expect(gestureLabel("trim-left", true, 1, [])).toBe("Stretch Clip");
    expect(gestureLabel("fade-in", false, 1, [{ id: "a", fade_in: fade(0) }])).toBe("Remove Fade In");
    expect(gestureLabel("trim-right", false, 2, [{ id: "a" }, { id: "b" }])).toBe("Trim 2 Clips");
  });
});

describe("drop ghost", () => {
  const view = { spp: 100, scroll: 0 };
  it("places files one after another", () => {
    const items = ghostItems([{ name: "a", len: 1_000 }, null, { name: "b", len: 200 }], 500, view, false);
    expect(items).toEqual([
      { name: "a", x: 5, w: 10 },
      { name: "b", x: 15, w: 4 },
    ]);
  });
  it("shows only the first when files go on tracks side by side", () => {
    expect(ghostItems([{ name: "a", len: 1_000 }, { name: "b", len: 1_000 }], 0, view, true)).toEqual([{ name: "a (+1 on the tracks below)", x: 0, w: 10 }]);
  });
});
