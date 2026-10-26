// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { sessionEnd, type ClipDoc, type Op, type SessionDoc } from "../../stores/session";
import { previewTracks } from "./clip-edit";
import { pointsOf } from "./envelope";
import { amplifyRange, clampRange, copyRange, cropTo, deleteRange, fadeDb, fadeRange, insertFile, mixPasteAt, pasteAt, replaceRange, silenceRange, stretchRange } from "./wave-edit";

const fade = () => ({ length: 0, shape: "linear" as const, curve: 0 });

/** A 10-second file at 1 kHz (10,000 samples) as one clip. */
function file(): SessionDoc {
  const clip: ClipDoc = {
    id: "c0",
    name: "f",
    media_id: "m",
    start: 0,
    source_in: 0,
    source_out: 10_000,
    gain_db: 0,
    muted: false,
    locked: false,
    color: null,
    fade_in: fade(),
    fade_out: fade(),
  };
  return {
    format: "altaudio.session",
    schema_version: 1,
    id: "s",
    name: "f",
    sample_rate: 1_000,
    bit_depth: 24,
    media: [{ id: "m", path: "f.wav", relative_path: null, sample_rate: 1_000, channels: 1, length: 10_000 }],
    tracks: [{ id: "t", name: "f", clips: [clip] }],
    markers: [],
    view: { samples_per_pixel: 10, scroll: 0, scroll_y: 0, playhead: 0, selection: null },
    libraries: [],
  } as unknown as SessionDoc;
}

/** Applies the clip ops the Waveform editor uses, like the core does. */
function apply(doc: SessionDoc, op: Op | null): SessionDoc {
  if (!op) return doc;
  if (op.op === "batch") return op.ops.reduce(apply, doc);
  const t = doc.tracks[0]!;
  if (op.op === "remove_clips") return { ...doc, tracks: [{ ...t, clips: t.clips.filter((c) => !op.ids.includes(c.id)) }] };
  if (op.op === "restore_clips") return { ...doc, tracks: [{ ...t, clips: [...t.clips, ...op.clips.map(([, c]) => c)] }] };
  if (op.op === "set_clips") return { ...doc, tracks: previewTracks(doc, op.changes) };
  if (op.op === "add_media") return { ...doc, media: [...doc.media, op.media] };
  if (op.op === "add_clips") {
    const made = op.clips.map((c) => ({ ...file().tracks[0]!.clips[0]!, ...c }));
    return { ...doc, tracks: [{ ...t, clips: [...t.clips, ...made] }] };
  }
  throw new Error(`not handled here: ${op.op}`);
}

/** The edit list as [start, source_in, source_out] in time order. */
const layout = (doc: SessionDoc) =>
  doc.tracks[0]!.clips
    .slice()
    .sort((a, b) => a.start - b.start)
    .map((c) => [c.start, c.source_in, c.source_out]);

let n = 0;
const id = () => `n${++n}`;

describe("Waveform editor edits", () => {
  it("Replace swaps a stretch for a new file of the same length", () => {
    const media = { id: "spec", path: "spectral-edit.wav", relative_path: null, sample_rate: 1_000, channels: 1, length: 2_000 };
    const doc = apply(file(), replaceRange(file(), 3_000, 5_000, media, "Spectral edit", id));
    expect(layout(doc)).toEqual([
      [0, 0, 3_000],
      [3_000, 0, 2_000],
      [5_000, 5_000, 10_000],
    ]);
    expect(doc.tracks[0]!.clips.find((c) => c.start === 3_000)!.media_id).toBe("spec");
    expect(sessionEnd(doc)).toBe(10_000);
  });

  it("Insert puts a take in at the playhead and moves the rest", () => {
    const media = { id: "take", path: "take.wav", relative_path: null, sample_rate: 1_000, channels: 1, length: 1_500 };
    const r = insertFile(file(), 4_000, media, "Take", id)!;
    const doc = apply(file(), r.op);
    expect(r.length).toBe(1_500);
    expect(layout(doc)).toEqual([
      [0, 0, 4_000],
      [4_000, 0, 1_500],
      [5_500, 4_000, 10_000],
    ]);
    expect(sessionEnd(doc)).toBe(11_500);
  });

  it("clamps and orders ranges", () => {
    expect(clampRange(file(), 8_000, 2_000)).toEqual([2_000, 8_000]);
    expect(clampRange(file(), 9_000, 20_000)).toEqual([9_000, 10_000]);
    expect(clampRange(file(), 12_000, 20_000)).toBeNull();
  });

  it("Delete closes the gap", () => {
    const doc = apply(file(), deleteRange(file(), 2_000, 5_000, id));
    expect(layout(doc)).toEqual([
      [0, 0, 2_000],
      [2_000, 5_000, 10_000],
    ]);
    expect(sessionEnd(doc)).toBe(7_000);
  });

  it("Crop keeps only the range, from 0", () => {
    const doc = apply(file(), cropTo(file(), 3_000, 6_000, id));
    expect(layout(doc)).toEqual([[0, 3_000, 6_000]]);
  });

  it("Crop works across earlier edits", () => {
    let doc = apply(file(), deleteRange(file(), 2_000, 5_000, id));
    doc = apply(doc, cropTo(doc, 1_000, 4_000, id));
    expect(layout(doc)).toEqual([
      [0, 1_000, 2_000],
      [1_000, 5_000, 7_000],
    ]);
  });

  it("Silence keeps the length and mutes only the range", () => {
    const doc = apply(file(), silenceRange(file(), 4_000, 6_000, id));
    expect(layout(doc)).toEqual([
      [0, 0, 4_000],
      [4_000, 4_000, 6_000],
      [6_000, 6_000, 10_000],
    ]);
    const muted = doc.tracks[0]!.clips.filter((c) => c.muted).map((c) => c.start);
    expect(muted).toEqual([4_000]);
  });

  it("Amplify adds gain to the range only", () => {
    let doc = apply(file(), amplifyRange(file(), 0, 5_000, 3, id));
    doc = apply(doc, amplifyRange(doc, 2_000, 8_000, -6, id));
    const gains = doc.tracks[0]!.clips
      .slice()
      .sort((a, b) => a.start - b.start)
      .map((c) => [c.start, c.gain_db]);
    expect(gains).toEqual([
      [0, 3],
      [2_000, -3],
      [5_000, -6],
      [8_000, 0],
    ]);
  });

  it("Fade In runs from silence to full across pieces", () => {
    expect(fadeDb(0)).toBe(-60);
    expect(fadeDb(1)).toBeCloseTo(0);
    expect(fadeDb(0.5)).toBeCloseTo(-3.01, 1);
    let doc = apply(file(), deleteRange(file(), 1_000, 2_000, id));
    doc = apply(doc, fadeRange(doc, 0, 2_000, "in", id));
    const clips = doc.tracks[0]!.clips.slice().sort((a, b) => a.start - b.start);
    const first = pointsOf(clips[0]!, "volume");
    const second = pointsOf(clips[1]!, "volume");
    expect(first[0]!.value).toBe(-60);
    expect(first[first.length - 1]!.value).toBeCloseTo(-3.01, 1);
    expect(second[0]!.value).toBeCloseTo(-3.01, 1);
    expect(second[second.length - 1]!.value).toBeCloseTo(0);
    expect(pointsOf(clips[2]!, "volume")).toEqual([]);
  });

  it("Copy and Paste insert the audio and push the rest right", () => {
    const board = copyRange(file(), 1_000, 3_000)!;
    expect(board.length).toBe(2_000);
    const r = pasteAt(file(), board, 5_000, id)!;
    const doc = apply(file(), r.op);
    expect(r.length).toBe(2_000);
    expect(layout(doc)).toEqual([
      [0, 0, 5_000],
      [5_000, 1_000, 3_000],
      [7_000, 5_000, 10_000],
    ]);
    expect(sessionEnd(doc)).toBe(12_000);
  });

  it("Paste from a file at another rate keeps its timing", () => {
    const board = { ...copyRange(file(), 0, 1_000)!, rate: 2_000 };
    const r = pasteAt(file(), board, 0, id)!;
    expect(r.length).toBe(500);
  });

  it("Mix Paste layers audio without moving anything", () => {
    const board = copyRange(file(), 0, 2_000)!;
    const r = mixPasteAt(file(), board, 9_000, -6, id)!;
    const doc = apply(file(), r.op);
    expect(layout(doc)).toEqual([
      [0, 0, 10_000],
      [9_000, 0, 2_000],
    ]);
    expect(doc.tracks[0]!.clips.find((c) => c.start === 9_000)!.gain_db).toBe(-6);
    expect(sessionEnd(doc)).toBe(11_000);
  });
});

import { markersAfterCrop, markersAfterCut, markersAfterInsert } from "./wave-edit";
import { newMarker } from "../markers/markers";

describe("markers follow Waveform edits", () => {
  const withMarks = (d: Parameters<typeof markersAfterCut>[0]) => ({
    ...d,
    markers: [newMarker("before", "b", 100), newMarker("inside", "i", 1_500), newMarker("after", "a", 5_000), newMarker("range", "r", 500, 2_000)],
  });
  it("a cut moves later markers left and keeps markers that were inside", () => {
    const [op] = markersAfterCut(withMarks(file()), 1_000, 2_000) as unknown as [{ changes: { id: string; position: number; duration?: number }[] }];
    expect(op.changes).toEqual([
      { id: "inside", position: 1_000, duration: 0 },
      { id: "after", position: 4_000, duration: 0 },
      { id: "range", position: 500, duration: 1_000 },
    ]);
  });
  it("an insert moves later markers right and grows ranges across it", () => {
    const [op] = markersAfterInsert(withMarks(file()), 1_000, 300) as unknown as [{ changes: object[] }];
    expect(op.changes).toEqual([
      { id: "inside", position: 1_800 },
      { id: "after", position: 5_300 },
      { id: "range", duration: 2_300 },
    ]);
  });
  it("a crop drops markers outside and moves the rest", () => {
    const ops = markersAfterCrop(withMarks(file()), 1_000, 3_000) as unknown as { op: string; ids?: string[]; changes?: object[] }[];
    expect(ops[0]).toEqual({ op: "remove_markers", ids: ["before", "after"] });
    expect(ops[1]!.changes).toEqual([
      { id: "inside", position: 500, duration: 0 },
      { id: "range", position: 0, duration: 1_500 },
    ]);
  });
});

describe("stretchRange", () => {
  const media = (length: number) => ({ id: "st", path: "st.wav", relative_path: null, sample_rate: 1_000, channels: 1, length });
  it("puts the stretched range in and moves what follows by the change", () => {
    const after = apply(file(), stretchRange(file(), 2_000, 5_000, media(2_000), "Stretch", id));
    const clips = after.tracks[0]!.clips.slice().sort((a, b) => a.start - b.start);
    expect(clips.map((c) => [c.start, c.source_in, c.source_out, c.media_id])).toEqual([
      [0, 0, 2_000, "m"],
      [2_000, 0, 2_000, "st"],
      [4_000, 5_000, 10_000, "m"],
    ]);
    expect(sessionEnd(after)).toBe(9_000);
  });
  it("makes room when the range gets longer", () => {
    const after = apply(file(), stretchRange(file(), 2_000, 5_000, media(4_000), "Stretch", id));
    expect(sessionEnd(after)).toBe(11_000);
  });
});
