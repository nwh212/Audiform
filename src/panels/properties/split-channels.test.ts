// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { ClipDoc, Op, SessionDoc, TrackDoc } from "../../stores/session";
import { splittable, splitChannelsOp } from "./split-channels";

const clip = (id: string, media: string): ClipDoc =>
  ({
    id,
    name: id,
    media_id: media,
    start: 100,
    source_in: 0,
    source_out: 500,
    gain_db: -3,
    muted: false,
    locked: false,
    color: null,
    fade_in: { length: 0, shape: "cosine", curve: 0 },
    fade_out: { length: 0, shape: "cosine", curve: 0 },
  }) as unknown as ClipDoc;

const track = (id: string, name: string, clips: ClipDoc[]): TrackDoc => ({ id, name, color: "#112233", clips }) as unknown as TrackDoc;

const doc = (channels: number): SessionDoc =>
  ({
    media: [{ id: "m", channels }],
    tracks: [track("t0", "Voice", [clip("c0", "m")]), track("t1", "Other", [])],
  }) as unknown as SessionDoc;

let n = 0;
const uuid = () => `id${++n}`;
const located = (d: SessionDoc) => [{ clip: d.tracks[0]!.clips[0]!, track: d.tracks[0]!, trackIndex: 0 }];
const flat = (op: Op | null): Op[] => (op === null ? [] : op.op === "batch" ? op.ops : [op]);

describe("split channels to tracks", () => {
  it("does nothing for a mono clip", () => {
    const d = doc(1);
    expect(splittable(d, located(d))).toEqual([]);
    expect(splitChannelsOp(d, located(d), uuid)).toBeNull();
  });

  it("makes one stereo track per channel right under the source track", () => {
    const d = doc(6);
    const ops = flat(splitChannelsOp(d, located(d), uuid));
    const adds = ops.filter((o) => o.op === "add_track");
    expect(adds.map((o) => (o.op === "add_track" ? o.name : ""))).toEqual(["Voice L", "Voice R", "Voice C", "Voice LFE", "Voice Ls", "Voice Rs"]);
    expect(adds.map((o) => (o.op === "add_track" ? o.index : -1))).toEqual([1, 2, 3, 4, 5, 6]);
  });

  it("plays one channel per clip, the way the original played it", () => {
    const d = doc(6);
    const ops = flat(splitChannelsOp(d, located(d), uuid));
    const sets = ops.filter((o) => o.op === "set_clips");
    const first = sets[0]!.op === "set_clips" ? sets[0]!.changes : [];
    expect(first).toHaveLength(6);
    // L straight to the left, everything else silent.
    expect(first[0]!.channel_mix!.sends[0]).toEqual({ left: 1, right: 0 });
    expect(first[0]!.channel_mix!.sends.slice(1).every((s) => s.left === 0 && s.right === 0)).toBe(true);
    // Every channel sums back to the default fold-down.
    const summed = first.reduce((acc, c) => acc + c.channel_mix!.sends.reduce((a, s) => a + s.left + s.right, 0), 0);
    expect(summed).toBeCloseTo(2 + 4 * Math.SQRT1_2, 5);
    expect(first[0]!.gain_db).toBe(-3);
  });

  it("mutes the LFE track and the original clip", () => {
    const d = doc(6);
    const ops = flat(splitChannelsOp(d, located(d), uuid));
    const muted = ops.filter((o) => o.op === "set_track" && o.changes.muted);
    expect(muted).toHaveLength(1);
    const last = ops[ops.length - 1]!;
    expect(last.op === "set_clips" && last.changes).toEqual([{ id: "c0", muted: true }]);
  });

  it("keeps the track color on the new tracks", () => {
    const d = doc(2);
    const ops = flat(splitChannelsOp(d, located(d), uuid));
    const colored = ops.filter((o) => o.op === "set_track" && o.changes.color === "#112233");
    expect(colored).toHaveLength(2);
  });
});
