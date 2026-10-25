// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { folderOf, insideAny, repointPlan, toCopy } from "./session-media";

describe("session media", () => {
  const session = "C:\\Shows\\Ep 1\\Ep 1.altaudio";

  it("finds the session folder and what is inside it", () => {
    expect(folderOf(session)).toBe("C:\\Shows\\Ep 1");
    expect(insideAny("c:/shows/ep 1/Recordings/take.wav", ["C:\\Shows\\Ep 1"], true)).toBe(true);
    expect(insideAny("C:\\Shows\\Ep 10\\a.wav", ["C:\\Shows\\Ep 1"], true)).toBe(false);
    expect(insideAny("/home/a/Ep/x.wav", ["/home/a/ep"], false)).toBe(false);
  });

  it("copies only files outside the session folder and the libraries left alone", () => {
    const paths = ["D:\\Audio\\vox.wav", "C:\\Shows\\Ep 1\\Audio\\vox.wav", "D:\\Lib\\Bumper.wav", "d:\\audio\\VOX.wav"];
    expect(toCopy(paths, session, ["D:\\Lib"], true)).toEqual(["D:\\Audio\\vox.wav"]);
    expect(toCopy(paths, session, [], true)).toEqual(["D:\\Audio\\vox.wav", "D:\\Lib\\Bumper.wav"]);
  });

  it("points the session and the Files panel at the copies", () => {
    const media = [
      { id: "m1", path: "D:\\Audio\\vox.wav", relative_path: "../../Audio/vox.wav", sample_rate: 48000, channels: 2, length: 10 },
      { id: "m2", path: "C:\\Shows\\Ep 1\\Recordings\\t.wav", relative_path: null, sample_rate: 48000, channels: 1, length: 5 },
    ];
    const pool = [
      { id: "m1", path: "D:\\Audio\\vox.wav" },
      { id: "f3", path: "D:\\Audio\\music.wav" },
    ];
    const moved = new Map([
      ["D:\\Audio\\vox.wav", "C:\\Shows\\Ep 1\\Audio\\vox.wav"],
      ["D:\\Audio\\music.wav", "C:\\Shows\\Ep 1\\Audio\\music.wav"],
    ]);
    const plan = repointPlan(media, pool, moved, true);
    expect(plan.ops).toEqual([{ op: "set_media", media: { ...media[0], path: "C:\\Shows\\Ep 1\\Audio\\vox.wav", relative_path: null } }]);
    expect(plan.pool.map((p) => p.id)).toEqual(["m1", "f3"]);
  });
});
