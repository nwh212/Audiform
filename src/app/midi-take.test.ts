// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { midiTakeClip } from "./midi-take";
import { DEFAULT_TEMPO } from "../stores/tempo";

describe("recorded MIDI", () => {
  // 120 bpm, 4/4: a beat is half a second, a bar two seconds.
  const rate = 48_000;
  it("becomes a clip on whole bars, notes placed in beats", () => {
    const notes = [
      { pitch: 60, channel: 0, velocity: 1, start: 0.5, end: 1.0 },
      { pitch: 64, channel: 2, velocity: 0.5, start: 2.25, end: 2.75 },
    ];
    // Recording started one bar in (2 seconds).
    const c = midiTakeClip(notes, 2 * rate, rate, DEFAULT_TEMPO, null, "c", "Take")!;
    expect([c.start, c.length]).toEqual([4, 8]);
    expect(c.notes[0]).toEqual({ pitch: 60, velocity: 127, start: 1, length: 1 });
    expect(c.notes[1]).toEqual({ pitch: 64, velocity: 64, start: 4.5, length: 1, channel: 2 });
  });
  it("keeps only notes starting inside a punch-in, and nothing makes no clip", () => {
    const notes = [
      { pitch: 60, channel: 0, velocity: 1, start: 0, end: 0.5 },
      { pitch: 62, channel: 0, velocity: 1, start: 3, end: 3.5 },
    ];
    const c = midiTakeClip(notes, 0, rate, DEFAULT_TEMPO, { start: 2 * rate, end: 4 * rate }, "c", "Take")!;
    expect(c.notes.map((n) => n.pitch)).toEqual([62]);
    expect(c.start).toBe(4);
    expect(midiTakeClip([], 0, rate, DEFAULT_TEMPO, null, "c", "Take")).toBeNull();
  });
});
