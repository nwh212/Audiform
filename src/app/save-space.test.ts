// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform.

import { describe, expect, it } from "vitest";
import { bytesText, spaceItems, spaceOps, type SpaceOutcome } from "./save-space";
import type { SessionDoc } from "../stores/session";

const clip = (id: string, media_id: string, extra: Record<string, unknown> = {}) => ({ id, media_id, ...extra });
const doc = (extra: Record<string, unknown> = {}) =>
  ({
    media: [
      { id: "a", path: "C:\\Show\\Audio\\a.wav", relative_path: null, sample_rate: 48000, channels: 2, length: 10 },
      { id: "b", path: "C:\\Show\\Audio\\b.wav", relative_path: null, sample_rate: 48000, channels: 2, length: 10 },
      { id: "c", path: "D:\\Elsewhere\\c.wav", relative_path: null, sample_rate: 48000, channels: 2, length: 10 },
      { id: "d", path: "C:\\Show\\Audio\\unused.wav", relative_path: null, sample_rate: 48000, channels: 2, length: 10 },
    ],
    tracks: [{ clips: [clip("1", "a"), clip("2", "b", { channel_mix: { gains: [] } }), clip("3", "c")] }],
    ...extra,
  }) as unknown as SessionDoc;

describe("Save Space", () => {
  it("takes used files inside the session folder, and keeps one-sided ones stereo", () => {
    const items = spaceItems(doc(), "C:\\Show\\show.audfrm", true);
    expect(items).toEqual([
      { path: "C:\\Show\\Audio\\a.wav", monoOk: true },
      { path: "C:\\Show\\Audio\\b.wav", monoOk: false },
    ]);
    // 3D Space: nothing becomes mono.
    expect(spaceItems(doc({ space: { layout: "5.1" } }), "C:\\Show\\show.audfrm", true).every((i) => !i.monoOk)).toBe(true);
    // A file left out on purpose.
    expect(spaceItems(doc(), "C:\\Show\\show.audfrm", true, ["c:/show/audio/a.wav"])).toHaveLength(1);
  });

  it("points the session at the copies, mono ones with one channel", () => {
    const out: SpaceOutcome[] = [
      { from: "C:\\Show\\Audio\\a.wav", to: "C:\\Show\\Audio\\a (mono).flac", bytesBefore: 10, bytesAfter: 3, flac: true, mono: true, channels: 1, skipped: null },
      { from: "C:\\Show\\Audio\\b.wav", to: null, bytesBefore: 10, bytesAfter: 10, flac: false, mono: false, channels: 0, skipped: "no" },
    ];
    const ops = spaceOps(doc().media, out, true);
    expect(ops).toEqual([{ op: "set_media", media: { id: "a", path: "C:\\Show\\Audio\\a (mono).flac", relative_path: null, sample_rate: 48000, channels: 1, length: 10 } }]);
  });

  it("writes sizes", () => {
    expect(bytesText(512)).toBe("512 bytes");
    expect(bytesText(2_400_000_000)).toBe("2.4 GB");
  });
});
