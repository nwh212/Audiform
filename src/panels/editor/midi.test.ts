// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

import { describe, expect, it } from "vitest";
import {
  addNote,
  beatsToSamples,
  clipAtBeat,
  duplicateClips,
  gridBeats,
  moveClips,
  moveNotes,
  newClip,
  noteName,
  removeNotes,
  resizeNotes,
  samplesToBeats,
  setVelocity,
  snap,
  snapDown,
} from "./midi";
import type { MidiClipDoc, NoteDoc } from "../../stores/session";

const t = { bpm: 120, beats_per_bar: 4 };
const clip = (id: string, start: number, length = 4): MidiClipDoc => ({ id, name: id, start, length, notes: [] });
const note = (pitch: number, start: number, length = 1): NoteDoc => ({ pitch, velocity: 100, start, length });

describe("midi", () => {
  it("turns beats into samples the way the core does", () => {
    // 120 BPM: a beat is half a second.
    expect(beatsToSamples(1, t, 48_000)).toBe(24_000);
    expect(beatsToSamples(0.25, t, 48_000)).toBe(6_000);
    expect(samplesToBeats(24_000, t, 48_000)).toBe(1);
    // Out-of-range tempos are held to the limits, as in the core.
    expect(beatsToSamples(1, { bpm: 1, beats_per_bar: 4 }, 48_000)).toBe(144_000);
  });

  it("snaps", () => {
    expect(snap(1.3, 0.5)).toBe(1.5);
    expect(snap(1.3, 0)).toBe(1.3);
    expect(snapDown(7.9, 4)).toBe(4);
    expect(gridBeats(0, { bpm: 90, beats_per_bar: 3 })).toBe(3);
    expect(gridBeats(-1, t)).toBe(0);
  });

  it("names notes", () => {
    expect(noteName(60)).toBe("C4");
    expect(noteName(69)).toBe("A4");
    expect(noteName(0)).toBe("C-1");
  });

  it("makes, finds, moves and copies clips", () => {
    expect(newClip("a", 5.5, t)).toMatchObject({ start: 4, length: 4 });
    const clips = [clip("a", 0), clip("b", 8)];
    expect(clipAtBeat(clips, 9)?.id).toBe("b");
    expect(clipAtBeat(clips, 5)).toBeNull();
    // Never before the start; order kept by start.
    const moved = moveClips(clips, new Set(["b"]), -20);
    expect(moved.map((c) => [c.id, c.start])).toEqual([
      ["a", 0],
      ["b", 0],
    ]);
    let n = 0;
    const d = duplicateClips(clips, new Set(["a", "b"]), () => `c${n++}`);
    expect(d.added).toEqual(["c0", "c1"]);
    expect(d.clips.map((c) => c.start)).toEqual([0, 8, 12, 20]);
  });

  it("edits notes", () => {
    let notes = addNote([], note(60, 2));
    notes = addNote(notes, note(64, 0));
    expect(notes.map((x) => x.pitch)).toEqual([64, 60]);
    // Moves keep inside the clip start and the pitch range.
    const m = moveNotes(notes, new Set([0, 1]), -5, 100);
    expect(m.map((x) => [x.start, x.pitch])).toEqual([
      [0, 127],
      [2, 123],
    ]);
    expect(resizeNotes(notes, new Set([0]), -5)[0]!.length).toBeGreaterThan(0);
    expect(setVelocity(notes, new Set([1]), 300)[1]!.velocity).toBe(127);
    expect(removeNotes(notes, new Set([0]))).toEqual([notes[1]]);
  });
});
