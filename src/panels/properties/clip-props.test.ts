// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { ClipDoc, MediaRefDoc } from "../../stores/session";
import { changeFor, settingsOf, varispeedSemitones } from "./clip-props";

const media: MediaRefDoc = { id: "m", path: "a.wav", relative_path: null, sample_rate: 48_000, channels: 1, length: 96_000 };
function clip(extra: Partial<ClipDoc> = {}): ClipDoc {
  return {
    id: "c",
    name: "c",
    media_id: "m",
    start: 0,
    source_in: 0,
    source_out: 48_000,
    gain_db: 0,
    muted: false,
    locked: false,
    color: null,
    fade_in: { length: 4_800, shape: "linear", curve: 0 },
    fade_out: { length: 0, shape: "linear", curve: 0 },
    envelopes: [],
    effects: { enabled: true, input_gain_db: 0, output_gain_db: 0, mix: 1, slots: [] },
    group: null,
    ...extra,
  } as ClipDoc;
}

describe("clip stretch and pitch settings", () => {
  it("reads a plain clip as off", () => {
    expect(settingsOf(clip())).toMatchObject({ mode: "off", type: "mono", percent: 100, pitch: 0 });
  });

  it("changes the pitch without changing the length", () => {
    const c = clip();
    const ch = changeFor(c, media, 48_000, { ...settingsOf(c), mode: "live", pitch: 3 })!;
    expect(ch.stretch).toMatchObject({ ratio: 1, keep_pitch: true, pitch: 3, polyphonic: false, rendered: false });
    expect(ch.fade_in).toBeUndefined();
  });

  it("stretches, scaling the fades, and keeps the pitch and type", () => {
    const c = clip({ stretch: { ratio: 1, keep_pitch: true, pitch: -2, polyphonic: true } });
    const ch = changeFor(c, media, 48_000, { ...settingsOf(c), percent: 150 })!;
    expect(ch.stretch).toMatchObject({ ratio: 1.5, pitch: -2, polyphonic: true });
    expect(ch.fade_in?.length).toBe(7_200);
  });

  it("varispeed ignores the pitch, and off removes the stretch", () => {
    const c = clip({ stretch: { ratio: 2, keep_pitch: true, pitch: 5 } });
    expect(changeFor(c, media, 48_000, { ...settingsOf(c), type: "varispeed" })!.stretch).toMatchObject({ keep_pitch: false, pitch: 0 });
    expect(changeFor(c, media, 48_000, { ...settingsOf(c), mode: "off" })!.stretch).toBeUndefined();
    expect(changeFor(c, media, 48_000, settingsOf(c))).not.toBeNull();
    expect(Math.round(varispeedSemitones(2))).toBe(-12);
  });
});
