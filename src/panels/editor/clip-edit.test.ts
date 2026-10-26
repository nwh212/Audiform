// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { ClipDoc, SessionDoc, TrackDoc } from "../../stores/session";
import { clipsAt, copyClips, copyMoved, fadeClip, gapAt, LockedError, MIN_CLIP, moveClips, nearest, pasteClips, previewTracks, rippleDeleteClips, rippleDeleteRange, snapTargets, trimToRange, splitClips, trimClip, trimToPlayhead, withCrossfades, withGroups, withLayers, TOP_LAYER, withVolume, withEnvelope, envelopeId, loopChanges } from "./clip-edit";

const fade = () => ({ length: 0, shape: "linear" as const, curve: 0 });

function clip(id: string, start: number, source_in = 0, source_out = 48_000, media_id = "m48"): ClipDoc {
  return { id, name: id, media_id, start, source_in, source_out, gain_db: 0, muted: false, locked: false, color: null, fade_in: fade(), fade_out: fade() };
}

function track(id: string, clips: ClipDoc[]): TrackDoc {
  return { id, name: id, clips } as unknown as TrackDoc;
}

function session(tracks: TrackDoc[]): SessionDoc {
  return {
    format: "altaudio.session",
    schema_version: 1,
    id: "s",
    name: "s",
    sample_rate: 48_000,
    bit_depth: 32,
    media: [
      { id: "m48", path: "a.wav", relative_path: null, sample_rate: 48_000, channels: 1, length: 480_000 },
      { id: "m44", path: "b.wav", relative_path: null, sample_rate: 44_100, channels: 1, length: 441_000 },
    ],
    tracks,
    markers: [],
    view: { samples_per_pixel: 100, scroll: 0, scroll_y: 0, playhead: 0, selection: null },
    libraries: [],
  };
}

describe("moveClips", () => {
  const doc = session([track("A", [clip("a", 10_000), clip("b", 100_000)]), track("B", [clip("c", 0)]), track("C", [])]);

  it("moves a group together and stops at the session start", () => {
    const r = moveClips(doc, ["a", "b"], -50_000, 0);
    expect(r.delta).toBe(-10_000);
    expect(r.changes).toEqual([
      { id: "a", start: 0 },
      { id: "b", start: 90_000 },
    ]);
  });

  it("moves between tracks within the track list", () => {
    const r = moveClips(doc, ["a"], 0, 5);
    expect(r.rows).toBe(2);
    expect(r.changes).toEqual([{ id: "a", track_id: "C" }]);
  });

  it("snaps the nearer edge of the group", () => {
    // Clip a ends at 58,000; clip c ends at 48,000. Dragging a left by
    // 9,000 puts its start at 1,000, which snaps to 0.
    const t = snapTargets(doc, new Set(["a"]), 0);
    expect(moveClips(doc, ["a"], -9_000, 0, t, 2_000).delta).toBe(-10_000);
  });

  it("leaves locked clips alone", () => {
    const locked = session([track("A", [{ ...clip("a", 0), locked: true }])]);
    expect(moveClips(locked, ["a"], 500, 0).changes).toEqual([]);
  });
});

describe("trimClip", () => {
  const doc = session([track("A", [clip("a", 10_000, 4_800, 48_000), clip("h", 0, 0, 44_100, "m44")])]);
  const a = doc.tracks[0]!.clips[0]!;

  it("left edge moves start and source together, stopping at the file start", () => {
    expect(trimClip(doc, a, "left", 1_000)).toEqual({ id: "a", start: 11_000, source_in: 5_800 });
    expect(trimClip(doc, a, "left", -9_000)).toEqual({ id: "a", start: 5_200, source_in: 0 });
  });

  it("right edge stops at the file end and keeps a minimum length", () => {
    expect(trimClip(doc, a, "right", 10_000_000)?.source_out).toBe(480_000);
    expect(trimClip(doc, a, "right", -1_000_000)?.source_out).toBe(4_800 + MIN_CLIP);
  });

  it("converts to the file's rate", () => {
    const h = doc.tracks[0]!.clips[1]!;
    expect(trimClip(doc, h, "right", -24_000)?.source_out).toBe(22_050);
  });
});

describe("fadeClip", () => {
  it("never lets the fades overlap and defaults new fades to straight", () => {
    const c = { ...clip("a", 0), fade_out: { length: 40_000, shape: "linear" as const, curve: 0 } };
    const doc = session([track("A", [c])]);
    expect(fadeClip(doc, c, "in", 20_000)).toEqual({ id: "a", fade_in: { length: 8_000, shape: "linear", curve: 0 } });
  });
});

describe("splitClips", () => {
  it("splits at the playhead and moves the fade out to the right part", () => {
    const c = { ...clip("a", 1_000), fade_in: { length: 100, shape: "linear" as const, curve: 0 }, fade_out: { length: 200, shape: "cosine" as const, curve: 0 } };
    const doc = session([track("A", [c])]);
    const op = splitClips(doc, ["a"], 25_000, () => "a2");
    expect(op).toMatchObject({
      op: "batch",
      ops: [
        { op: "set_clips", changes: [{ id: "a", source_out: 24_000, fade_out: { length: 0 } }] },
        { op: "restore_clips", clips: [["A", { id: "a2", start: 25_000, source_in: 24_000, source_out: 48_000, fade_in: { length: 0 }, fade_out: { length: 200 } }]] },
      ],
    });
    expect(splitClips(doc, ["a"], 500, () => "x")).toBeNull();
  });

  it("splits the selected clips under the playhead, or all of them when none are selected", () => {
    const doc = session([track("A", [clip("a", 0)]), track("B", [clip("b", 0)])]);
    expect(clipsAt(doc, 100, new Set(["b"]))).toEqual(["b"]);
    expect(clipsAt(doc, 100, new Set())).toEqual(["a", "b"]);
  });
});

describe("trimToPlayhead", () => {
  it("trims start or end to the playhead", () => {
    const doc = session([track("A", [clip("a", 0)])]);
    expect(trimToPlayhead(doc, ["a"], 12_000, "start")).toEqual([{ id: "a", start: 12_000, source_in: 12_000 }]);
    expect(trimToPlayhead(doc, ["a"], 12_000, "end")).toEqual([{ id: "a", source_out: 12_000 }]);
  });
});

describe("nearest", () => {
  it("picks the closest target in range", () => {
    expect(nearest(100, [0, 90, 120], 15)).toBe(90);
    expect(nearest(100, [0, 200], 15)).toBeNull();
  });
});

describe("previewTracks", () => {
  it("shows moved and trimmed clips where they will land", () => {
    const doc = session([track("A", [clip("a", 0), clip("b", 50_000)]), track("B", [])]);
    const t = previewTracks(doc, [
      { id: "a", track_id: "B", start: 5 },
      { id: "b", source_out: 100 },
    ]);
    expect(t[0]!.clips.map((c) => [c.id, c.source_out])).toEqual([["b", 100]]);
    expect(t[1]!.clips.map((c) => [c.id, c.start])).toEqual([["a", 5]]);
  });
});

describe("rippleDeleteRange", () => {
  it("removes, trims, splits and shifts on the chosen tracks", () => {
    // Both clips that start before 20k end after 30k, so both are split.
    const doc = session([
      track("A", [clip("left", 0), clip("after", 60_000)]),
      track("B", [clip("span", 10_000)]),
      track("C", [clip("untouched", 50_000)]),
    ]);
    const op = rippleDeleteRange(doc, ["A", "B"], 20_000, 30_000, () => "new");
    expect(op).toMatchObject({
      op: "batch",
      ops: [
        {
          op: "set_clips",
          changes: [
            { id: "left", source_out: 20_000 },
            { id: "after", start: 50_000 },
            { id: "span", source_out: 10_000 },
          ],
        },
        {
          op: "restore_clips",
          clips: [
            ["A", { id: "new", start: 20_000, source_in: 30_000, source_out: 48_000 }],
            ["B", { id: "new", start: 20_000, source_in: 20_000, source_out: 48_000 }],
          ],
        },
      ],
    });
  });

  it("refuses when a locked clip would change", () => {
    const doc = session([track("A", [{ ...clip("a", 50_000), locked: true }])]);
    expect(() => rippleDeleteRange(doc, ["A"], 0, 10, () => "x")).toThrow(LockedError);
  });
});

describe("rippleDeleteClips", () => {
  it("closes the space on the same track only", () => {
    const doc = session([track("A", [clip("a", 0), clip("b", 48_000), clip("c", 100_000)]), track("B", [clip("d", 100_000)])]);
    expect(rippleDeleteClips(doc, ["a"])).toEqual({
      op: "batch",
      ops: [
        { op: "remove_clips", ids: ["a"] },
        {
          op: "set_clips",
          changes: [
            { id: "b", start: 0 },
            { id: "c", start: 52_000 },
          ],
        },
      ],
    });
  });
});

describe("gaps, ranges and the clipboard", () => {
  const doc = session([track("A", [clip("a", 0), clip("b", 60_000)]), track("B", [])]);

  it("finds the gap around a point", () => {
    expect(gapAt(doc, "A", 50_000)).toEqual({ start: 48_000, end: 60_000 });
    expect(gapAt(doc, "A", 10)).toBeNull();
    expect(gapAt(doc, "A", 200_000)).toBeNull();
  });

  it("trims clips to a range", () => {
    expect(trimToRange(doc, ["a", "b"], 10_000, 70_000)).toEqual([
      { id: "a", start: 10_000, source_in: 10_000 },
      { id: "b", source_out: 10_000 },
    ]);
  });

  it("copies and pastes clips keeping their layout", () => {
    const board = copyClips(doc, ["a", "b"])!;
    let n = 0;
    const r = pasteClips(doc, board, 1_000, 1, () => `p${++n}`)!;
    expect(r.op).toMatchObject({
      op: "restore_clips",
      clips: [
        ["B", { id: "p1", start: 1_000 }],
        ["B", { id: "p2", start: 61_000 }],
      ],
    });
  });
});

describe("rippleDeleteRange edges", () => {
  it("deletes clips inside and pulls in clips crossing the right edge", () => {
    const doc = session([track("A", [clip("inside", 21_000, 0, 5_000), clip("right", 25_000)])]);
    expect(rippleDeleteRange(doc, ["A"], 20_000, 30_000, () => "x")).toEqual({
      op: "batch",
      ops: [
        { op: "remove_clips", ids: ["inside"] },
        { op: "set_clips", changes: [{ id: "right", start: 20_000, source_in: 5_000, fade_in: { length: 0, shape: "linear", curve: 0 } }] },
      ],
    });
  });
});

describe("withCrossfades", () => {
  it("fades across the overlap a move creates", () => {
    const doc = session([track("A", [clip("a", 0), clip("b", 100_000)])]);
    const r = withCrossfades(doc, [{ id: "b", start: 40_000 }]);
    expect(r).toEqual([
      { id: "b", start: 40_000, fade_in: { length: 8_000, shape: "linear", curve: 0 } },
      { id: "a", fade_out: { length: 8_000, shape: "linear", curve: 0 } },
    ]);
  });

  it("takes the crossfade away when a clip moves wholly inside the other", () => {
    const fade = (length: number) => ({ length, shape: "cosine" as const, curve: 0 });
    const a = { ...clip("a", 0), fade_out: fade(8_000) };
    const b = { ...clip("b", 40_000), fade_in: fade(8_000) };
    const doc = session([track("A", [a, b])]);
    // b overlapped a's end by 8,000; now it lies exactly over a (same size), so it simply covers it.
    const r = withCrossfades(doc, [{ id: "b", start: 0 }]);
    expect(r).toContainEqual({ id: "b", start: 0, fade_in: fade(0) });
    expect(r).toContainEqual({ id: "a", fade_out: fade(0) });
  });

  it("leaves untouched and nested clips alone", () => {
    const doc = session([track("A", [clip("a", 0), clip("b", 10_000, 0, 1_000)])]);
    expect(withCrossfades(doc, [{ id: "b", start: 20_000 }])).toEqual([{ id: "b", start: 20_000 }]);
  });
});

describe("withGroups", () => {
  it("adds the rest of each group", () => {
    const doc = session([track("A", [{ ...clip("a", 0), group: "g" }, clip("b", 50_000)]), track("B", [{ ...clip("c", 0), group: "g" }])]);
    expect(withGroups(doc, ["a"]).sort()).toEqual(["a", "c"]);
    expect(withGroups(doc, ["b"])).toEqual(["b"]);
  });
});

describe("volume envelopes through edits", () => {
  it("split gives each part the levels it had", () => {
    const pts = [
      { position: 0, value: 0, curve: "linear" as const },
      { position: 40_000, value: -20, curve: "linear" as const },
    ];
    const c = withVolume(clip("a", 0), pts);
    const doc = session([track("A", [c])]);
    const op = splitClips(doc, ["a"], 20_000, () => "a2") as unknown as { ops: [{ changes: { volume: unknown }[] }, { clips: [string, ClipDoc][] }] };
    expect(op.ops[0].changes[0]!.volume).toEqual([
      { position: 0, value: 0, curve: "linear" },
      { position: 20_000, value: -10, curve: "linear" },
    ]);
    const right = op.ops[1].clips[0]![1];
    expect(right.envelopes?.[0]?.id).toBe("a2");
    expect(right.envelopes?.[0]?.points[0]).toEqual({ position: 0, value: -10, curve: "linear" });
  });

  it("a left trim moves the points with the audio", () => {
    const c = withVolume(clip("a", 0), [{ position: 10_000, value: -6, curve: "linear" }]);
    const doc = session([track("A", [c])]);
    const ch = trimClip(doc, c, "left", 4_000)!;
    expect(ch.volume?.[0]).toEqual({ position: 0, value: -6, curve: "linear" });
  });
});

describe("pan envelopes through edits", () => {
  const id = "0b9f6a52-3c1e-4d2b-8f00-5a1d2c3e4f50";

  it("uses the core's pan envelope id", () => {
    expect(envelopeId(id, "volume")).toBe(id);
    expect(envelopeId(id, "pan")).toBe("0b9f6a52-3c1e-4d2b-8f00-5a1d2c3e4f51");
    expect(envelopeId(envelopeId(id, "pan"), "pan")).toBe(id);
  });

  it("split carries pan and volume to both parts", () => {
    let c = withVolume(clip("a", 0), [{ position: 0, value: -6, curve: "linear" }]);
    c = withEnvelope(c, "pan", [
      { position: 0, value: -100, curve: "linear" },
      { position: 40_000, value: 100, curve: "linear" },
    ]);
    const doc = session([track("A", [c])]);
    const op = splitClips(doc, ["a"], 20_000, () => "a2") as unknown as {
      ops: [{ changes: { volume?: unknown; pan?: { value: number }[] }[] }, { clips: [string, ClipDoc][] }];
    };
    const left = op.ops[0].changes[0]!;
    expect(left.volume).toBeDefined();
    expect(left.pan?.[left.pan.length - 1]?.value).toBe(0);
    const right = op.ops[1].clips[0]![1];
    const pan = right.envelopes?.find((e) => e.target.type === "pan");
    expect(pan?.points[0]?.value).toBe(0);
    expect(right.envelopes?.some((e) => e.target.type === "volume")).toBe(true);
  });

  it("preview applies a pan change", () => {
    const doc = session([track("A", [clip("a", 0)])]);
    const t = previewTracks(doc, [{ id: "a", pan: [{ position: 0, value: 50, curve: "linear" }] }]);
    expect(t[0]!.clips[0]!.envelopes?.[0]?.target).toEqual({ type: "pan" });
  });
});

describe("looped clips", () => {
  // Region is 48,000 samples (one second) of a ten-second file.
  const looped = (): ClipDoc => ({ ...clip("a", 0, 0, 48_000), looped: { length: 48_000, phase: 0 } });

  it("Loop Clip turns on, then off back to the region played once", () => {
    const doc = session([track("A", [clip("a", 0, 0, 48_000)])]);
    const on = loopChanges(doc, ["a"]);
    expect(on.on).toBe(true);
    expect(on.changes).toEqual([{ id: "a", looped: { length: 48_000, phase: 0 } }]);

    const lc = { ...clip("a", 0, 0, 48_000), looped: { length: 200_000, phase: 12_000 } };
    const off = loopChanges(session([track("A", [lc])]), ["a"]);
    expect(off.on).toBe(false);
    expect(off.changes[0]).toMatchObject({ unloop: true, source_in: 12_000, source_out: 48_000 });
  });

  it("trims right past the end of the audio", () => {
    const c = looped();
    const doc = session([track("A", [c])]);
    const ch = trimClip(doc, c, "right", 1_000_000)!;
    expect(ch.looped).toEqual({ length: 1_048_000, phase: 0 });
    expect(ch.source_out).toBeUndefined();
  });

  it("left trims move the phase, both ways", () => {
    const c = { ...looped(), start: 100_000, looped: { length: 96_000, phase: 0 } };
    const doc = session([track("A", [c])]);
    expect(trimClip(doc, c, "left", 10_000)!.looped).toEqual({ length: 86_000, phase: 10_000 });
    expect(trimClip(doc, c, "left", -10_000)!.looped).toEqual({ length: 106_000, phase: 38_000 });
  });

  it("split keeps each side's place in the loop", () => {
    const c = { ...looped(), looped: { length: 120_000, phase: 6_000 } };
    const doc = session([track("A", [c])]);
    const op = splitClips(doc, ["a"], 50_000, () => "a2") as unknown as { ops: [{ changes: ClipDoc[] }, { clips: [string, ClipDoc][] }] };
    expect(op.ops[0].changes[0]!.looped).toEqual({ length: 50_000, phase: 6_000 });
    const right = op.ops[1].clips[0]![1];
    expect(right.looped).toEqual({ length: 70_000, phase: 8_000 });
    expect(right.source_in).toBe(0);
    expect(right.source_out).toBe(48_000);
  });

  it("previewing an unloop clears the loop", () => {
    const doc = session([track("A", [looped()])]);
    const t = previewTracks(doc, [{ id: "a", unloop: true }]);
    expect(t[0]!.clips[0]!.looped).toBeNull();
  });
});

import { insertTime } from "./clip-edit";

describe("insertTime", () => {
  it("splits a clip at the point and pushes everything after it", () => {
    const doc = session([track("A", [clip("a", 0), clip("b", 100_000)]), track("B", [clip("c", 60_000)])]);
    let n = 0;
    const op = insertTime(doc, ["A", "B"], 24_000, 48_000, () => `n${++n}`)!;
    expect(op.op).toBe("batch");
    const ops = (op as { ops: { op: string }[] }).ops;
    expect(ops[0]!.op).toBe("batch"); // the split
    const moves = ops.find((o) => o.op === "set_clips") as unknown as { changes: { id: string; start: number }[] };
    expect(moves.changes).toEqual(
      expect.arrayContaining([
        { id: "n1", start: 72_000 },
        { id: "b", start: 148_000 },
        { id: "c", start: 108_000 },
      ]),
    );
  });
  it("only moves the chosen tracks, and refuses locked clips in the way", () => {
    const doc = session([track("A", [clip("b", 100_000)]), track("B", [{ ...clip("c", 60_000), locked: true }])]);
    const op = insertTime(doc, ["A"], 24_000, 48_000, () => "x")!;
    expect(JSON.stringify(op)).not.toContain('"c"');
    expect(() => insertTime(doc, ["A", "B"], 24_000, 48_000, () => "x")).toThrow(LockedError);
  });
});

import { clearRange } from "./clip-edit";

describe("clearRange", () => {
  it("leaves silence in the range without moving later audio", () => {
    const doc = session([track("A", [clip("a", 0), clip("b", 100_000)])]);
    let n = 0;
    const op = clearRange(doc, ["A"], 12_000, 24_000, () => `n${++n}`)!;
    const json = JSON.stringify(op);
    expect(json).toContain("remove_clips");
    // Clip b, after the range, is not touched.
    expect(json).not.toContain('"b"');
  });
  it("does nothing on empty time", () => {
    const doc = session([track("A", [clip("b", 100_000)])]);
    expect(clearRange(doc, ["A"], 0, 24_000, () => "x")).toBeNull();
  });
});

describe("copyMoved", () => {
  it("copies the dragged clips to where they were dropped and leaves the originals", () => {
    const clip = (id: string, start: number, group?: string) => ({ id, name: id, media_id: "m", start, source_in: 0, source_out: 100, gain_db: 0, muted: false, locked: false, color: null, fade_in: { length: 0, shape: "linear", curve: 0 }, fade_out: { length: 0, shape: "linear", curve: 0 }, ...(group ? { group } : {}) });
    const doc = {
      tracks: [
        { id: "t1", clips: [clip("a", 0, "g"), clip("b", 200, "g"), clip("n", 500)] },
        { id: "t2", clips: [] },
      ],
    } as unknown as SessionDoc;
    let k = 0;
    const r = copyMoved(doc, ["a", "b"], [{ id: "a", start: 50, track_id: "t2" }, { id: "b", start: 250, track_id: "t2" }, { id: "n", fade_in: { length: 10, shape: "linear", curve: 0 } }], () => `new${k++}`);
    expect(r.ids).toEqual(["new0", "new2"]);
    const ops = (r.op as { op: "batch"; ops: { op: string; clips?: [string, { id: string; start: number; group: string | null }][] }[] }).ops;
    expect(ops[0]!.clips!.map(([t, c]) => [t, c.id, c.start, c.group])).toEqual([
      ["t2", "new0", 50, "new1"],
      ["t2", "new2", 250, "new1"],
    ]);
    expect(ops[1]).toEqual({ op: "set_clips", changes: [{ id: "n", fade_in: { length: 10, shape: "linear", curve: 0 } }] });
  });
});

describe("withLayers", () => {
  it("puts a dragged clip on top of what it lands on", () => {
    const doc = session([track("A", [clip("a", 200_000), clip("b", 0)])]);
    const r = withLayers(doc, [{ id: "a", start: 10_000 }]);
    expect(r).toEqual([{ id: "a", start: 10_000, layer: TOP_LAYER }]);
    expect(previewTracks(doc, r)[0]!.clips.map((c) => c.id)).toEqual(["b", "a"]);
  });

  it("leaves a clip tucked under the same clip where it is", () => {
    const doc = session([track("A", [clip("a", 10_000), clip("b", 0)])]);
    expect(withLayers(doc, [{ id: "a", start: 12_000 }])).toEqual([{ id: "a", start: 12_000 }]);
  });
});
