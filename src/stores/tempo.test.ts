// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { addChange, barAt, barStart, beatsToSeconds, editSegment, moveChange, nearestBar, parseMeter, removeChange, secondsToBeats, segmentAtSeconds, segments } from "./tempo";

const map = { bpm: 120, beats_per_bar: 4, changes: [{ id: "a", bar: 2, bpm: 60, beats_per_bar: 6, beat_unit: 8 }] };

describe("tempo map", () => {
  it("lays out stretches from bar lines", () => {
    const s = segments(map);
    expect(s.map((x) => [x.bar, x.beat, x.seconds])).toEqual([
      [0, 0, 0],
      [2, 8, 4],
    ]);
    // A bar of 6/8 is three quarter notes.
    expect(barStart(map, 3)).toBe(11);
    expect(beatsToSeconds(map, 11)).toBe(7);
    expect(secondsToBeats(map, 7)).toBe(11);
    expect(barAt(map, 9.5)).toMatchObject({ bar: 2, beat: 3 });
    expect(nearestBar(map, 9)).toBe(2);
    expect(nearestBar(map, 10)).toBe(3);
  });

  it("edits the stretch at the playhead, and adds, moves and removes changes", () => {
    const start = segmentAtSeconds(map, 1);
    expect(editSegment(map, start, { bpm: 90 }).bpm).toBe(90);
    const later = segmentAtSeconds(map, 5);
    expect(editSegment(map, later, { beats_per_bar: 7 }).changes?.[0]?.beats_per_bar).toBe(7);
    const added = addChange(map, 5, "b");
    expect(added.tempo.changes?.map((c) => [c.bar, c.bpm])).toEqual([
      [2, 60],
      [5, 60],
    ]);
    // Adding where there is one already picks that one.
    expect(addChange(map, 2, "c")).toEqual({ tempo: map, id: "a" });
    expect(moveChange(added.tempo, "a", 5).changes?.map((c) => c.id)).toEqual(["a"]);
    expect(removeChange(map, "a").changes).toEqual([]);
  });

  it("reads time signatures", () => {
    expect(parseMeter("6/8")).toEqual({ beats_per_bar: 6, beat_unit: 8 });
    expect(parseMeter(" 3 ")).toEqual({ beats_per_bar: 3, beat_unit: 4 });
    expect(parseMeter("4/5")).toBeNull();
    expect(parseMeter("x")).toBeNull();
  });
});
