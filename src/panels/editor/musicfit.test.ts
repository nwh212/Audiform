// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { ClipDoc, MediaRefDoc, SessionDoc } from "../../stores/session";
import { describeJoins, describePlan, FIT_XFADE, fitOps } from "./musicfit";

const media = { id: "m", path: "bed.wav", sample_rate: 44_100, channels: 2, length: 44_100 * 32 } as unknown as MediaRefDoc;
const clip = {
  id: "c",
  name: "Bed",
  media_id: "m",
  start: 48_000,
  source_in: 0,
  source_out: 44_100 * 32,
  gain_db: -6,
  muted: false,
  locked: false,
  color: null,
  fade_in: { length: 4800, shape: "linear", curve: 0 },
  fade_out: { length: 9600, shape: "linear", curve: 0 },
} as unknown as ClipDoc;
const doc = { sample_rate: 48_000 } as unknown as SessionDoc;

let n = 0;
const id = () => `n${n++}`;

describe("music fit", () => {
  it("lays segments end to end with a crossfade at each join, keeping the outer fades", () => {
    const plan = { segments: [{ start: 0, end: 10.02 }, { start: 21.98, end: 32 }], length: 20, stretch: 1, joins: 1, quality: 0.97, tempo: 120 };
    const op = fitOps(doc, "t", clip, media, plan, id);
    expect(op.op).toBe("batch");
    const ops = (op as { ops: unknown[] }).ops as [{ op: string; ids: string[] }, { op: string; clips: [string, ClipDoc][] }];
    expect(ops[0]).toEqual({ op: "remove_clips", ids: ["c"] });
    const [a, b] = ops[1].clips.map((x) => x[1]);
    const xf = Math.round(FIT_XFADE * 48_000);
    expect(a!.start).toBe(48_000);
    expect(a!.source_in).toBe(0);
    expect(a!.fade_in.length).toBe(4800);
    expect(a!.fade_out.length).toBe(xf);
    expect(b!.fade_in.length).toBe(xf);
    expect(b!.fade_out.length).toBe(9600);
    // The second starts where the first ends, less the crossfade.
    const aLen = Math.round(((a!.source_out - a!.source_in) * 48_000) / 44_100);
    expect(b!.start).toBe(48_000 + aLen - xf);
    // Total: 20 s of music.
    const bLen = Math.round(((b!.source_out - b!.source_in) * 48_000) / 44_100);
    expect((b!.start + bLen - 48_000) / 48_000).toBeCloseTo(20, 2);
    // Grouped, and the clip's gain carries over.
    expect(a!.group).toBeTruthy();
    expect(a!.group).toBe(b!.group);
    expect(b!.gain_db).toBe(-6);
  });

  it("applies the small stretch and describes the plan", () => {
    const plan = { segments: [{ start: 0, end: 32 }], length: 32, stretch: 1.01, joins: 0, quality: 1, tempo: 119.6 };
    const op = fitOps(doc, "t", clip, media, plan, id) as unknown as { ops: [unknown, { clips: [string, ClipDoc][] }] };
    expect(op.ops[1].clips[0]![1].stretch).toEqual({ ratio: 1.01, keep_pitch: true });
    expect(op.ops[1].clips[0]![1].id).toBe("c");
    expect(describePlan(plan)).toBe("No joins, 100% match, 120 BPM, stretched 1.0%");
  });
  it("says where a plan joins, in clip time", () => {
    const h = FIT_XFADE / 2;
    const skip = { segments: [{ start: 10, end: 52 + h }, { start: 80 - h, end: 100 }], length: 0, stretch: 1, joins: 1, quality: 1, tempo: 120 };
    expect(describeJoins(skip, 10)).toBe("Cuts 42.0 s to 1:10.0");
    const loop = { segments: [{ start: 0, end: 46 + h }, { start: 30 - h, end: 46 + h }, { start: 30 - h, end: 60 }], length: 0, stretch: 1, joins: 2, quality: 1, tempo: 120 };
    expect(describeJoins(loop, 0)).toBe("Repeats 30.0 s to 46.0 s, 2 more times");
    expect(describeJoins({ ...skip, segments: [{ start: 0, end: 5 }] }, 0)).toBe("Plays the whole clip");
  });
});
