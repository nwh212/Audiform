// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import {
  transferClips,
  addNote,
  beatsToSamples,
  clipAtBeat,
  duplicateClips,
  gridBeats,
  moveClips,
  moveNotes,
  newClip,
  noteName,
  copyNotes,
  pasteNotes,
  resizeNotesStart,
  spanOf,
  removeNotes,
  resizeNotes,
  samplesToBeats,
  setVelocity,
  snap,
  snapDown,
  stretchClip,
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

  it("stretches note starts, copies and pastes", () => {
    const notes = [note(60, 1, 2), note(64, 2, 1)];
    // Dragging the start left lengthens, right shortens, the end stays.
    expect(resizeNotesStart(notes, new Set([0]), -1)[0]).toMatchObject({ start: 0, length: 3 });
    expect(resizeNotesStart(notes, new Set([0]), 5)[0]!.start + resizeNotesStart(notes, new Set([0]), 5)[0]!.length).toBe(3);
    expect(resizeNotesStart(notes, new Set([0]), -9)[0]).toMatchObject({ start: 0, length: 3 });
    const copied = copyNotes(notes, new Set([0, 1]));
    expect(copied.map((n) => n.start)).toEqual([0, 1]);
    expect(spanOf(copied)).toBe(2);
    const p = pasteNotes(notes, copied, 4);
    expect(p.notes.map((n) => [n.pitch, n.start])).toEqual([
      [60, 1],
      [64, 2],
      [60, 4],
      [64, 5],
    ]);
    expect([...p.pasted].sort()).toEqual([2, 3]);
  });
});

describe("moving MIDI clips between tracks", () => {
  const clip = (id: string, start: number) => ({ id, name: id, start, length: 4, notes: [{ pitch: 60, velocity: 100, start: 0, length: 1 }] });
  it("moves clips with their notes, keeping their ids", () => {
    const r = transferClips([clip("a", 0), clip("b", 8)], [clip("c", 4)], new Set(["a"]), 2, false, () => "new");
    expect(r.from.map((c) => c.id)).toEqual(["b"]);
    expect(r.to.map((c) => [c.id, c.start])).toEqual([["a", 2], ["c", 4]]);
    expect(r.to[0]!.notes).toHaveLength(1);
  });
  it("copies under new ids and leaves the source alone, never before the start", () => {
    const r = transferClips([clip("a", 1)], [], new Set(["a"]), -5, true, () => "new");
    expect(r.from.map((c) => c.id)).toEqual(["a"]);
    expect(r.to.map((c) => [c.id, c.start])).toEqual([["new", 0]]);
  });
});

describe("moving clips between tracks", () => {
  it("works on clips held as live editor state (proxies), moving and copying", async () => {
    const { transferClips } = await import("./midi");
    const clip = { id: "c1", name: "A", start: 4, length: 4, notes: [{ pitch: 60, velocity: 100, start: 0, length: 1 }] };
    // A proxy, as the editor's state is: some copy functions refuse these.
    const live = new Proxy(clip, {});
    const moved = transferClips([live], [], new Set(["c1"]), 2, false, () => "new");
    expect(moved.from).toEqual([]);
    expect(moved.to.map((c) => [c.id, c.start])).toEqual([["c1", 6]]);
    const copied = transferClips([live], [], new Set(["c1"]), 0, true, () => "new");
    expect(copied.from).toHaveLength(1);
    expect(copied.to.map((c) => c.id)).toEqual(["new"]);
  });
});

describe("note tools", () => {
  const n = (start: number, velocity = 100) => ({ pitch: 60, velocity, start, length: 0.5 });
  const notes = [n(0), n(1, 50), n(2, 120)];
  it("acts on every note when none are picked", async () => {
    const { targetsOf } = await import("./midi");
    expect([...targetsOf(notes, new Set())]).toEqual([0, 1, 2]);
    expect([...targetsOf(notes, new Set([1]))]).toEqual([1]);
  });
  it("scales and randomizes velocity within 1 to 127", async () => {
    const { scaleVelocity, randomizeVelocity } = await import("./midi");
    expect(scaleVelocity(notes, new Set([0, 2]), 150).map((x) => x.velocity)).toEqual([127, 50, 127]);
    expect(randomizeVelocity(notes, new Set([1]), 10, () => 1).map((x) => x.velocity)).toEqual([100, 60, 120]);
    expect(randomizeVelocity(notes, new Set([1]), 100, () => 0)[1]!.velocity).toBe(1);
  });
  it("pans, humanizes and sets channels", async () => {
    const { randomizePan, humanize, setChannel } = await import("./midi");
    expect(randomizePan(notes, new Set([0]), 0.5, () => 0)[0]!.pan).toBe(-0.5);
    const h = humanize(notes, new Set([0, 1]), 0.1, () => 0);
    expect(h.map((x) => x.start)).toEqual([0, 0.9, 2]);
    expect(setChannel(notes, new Set([2]), 20)[2]!.channel).toBe(15);
  });
});

describe("stretching a MIDI clip", () => {
  const clip = (): MidiClipDoc =>
    ({
      id: "c",
      name: "Riff",
      start: 4,
      length: 4,
      notes: [
        { pitch: 60, velocity: 100, start: 0, length: 1 },
        { pitch: 64, velocity: 90, start: 2, length: 0.5, channel: 3 },
      ],
    }) as MidiClipDoc;

  it("spreads the notes out with a longer clip, keeping pitch, velocity and channel", () => {
    const [c] = stretchClip([clip()], "c", 8);
    expect(c!.length).toBe(8);
    expect(c!.start).toBe(4);
    expect(c!.notes.map((n) => [n.start, n.length])).toEqual([
      [0, 2],
      [4, 1],
    ]);
    expect(c!.notes[1]).toMatchObject({ pitch: 64, velocity: 90, channel: 3 });
  });

  it("squeezes them together with a shorter one, and leaves other clips alone", () => {
    const other = { ...clip(), id: "d" };
    const out = stretchClip([clip(), other], "c", 2);
    expect(out[0]!.notes.map((n) => [n.start, n.length])).toEqual([
      [0, 0.5],
      [1, 0.25],
    ]);
    expect(out[1]).toBe(other);
  });
});
