// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { AudioInfo } from "../../app/core";
import type { MediaItem } from "../../stores/media-pool";
import { formatChannels, formatDuration, formatRate, formatSize, matches, sortItems } from "./format";
import { clickSelection, stepSelection } from "./selection";

const ids = ["a", "b", "c", "d", "e"];
const none = new Set<string>();

describe("list selection", () => {
  it("plain click selects one", () => {
    expect(clickSelection(new Set(["a", "b"]), "a", "c", ids, { ctrl: false, shift: false })).toEqual({
      selected: ["c"],
      anchor: "c",
    });
  });

  it("ctrl click toggles", () => {
    expect(clickSelection(new Set(["a"]), "a", "c", ids, { ctrl: true, shift: false }).selected).toEqual(["a", "c"]);
    expect(clickSelection(new Set(["a", "c"]), "a", "c", ids, { ctrl: true, shift: false }).selected).toEqual(["a"]);
  });

  it("shift click selects a range from the anchor, either direction", () => {
    expect(clickSelection(none, "b", "d", ids, { ctrl: false, shift: true }).selected).toEqual(["b", "c", "d"]);
    expect(clickSelection(none, "d", "b", ids, { ctrl: false, shift: true }).selected).toEqual(["b", "c", "d"]);
  });

  it("ctrl+shift adds the range to the selection", () => {
    const r = clickSelection(new Set(["e"]), "a", "b", ids, { ctrl: true, shift: true });
    expect(r.selected.sort()).toEqual(["a", "b", "e"]);
  });

  it("arrow keys move and stop at the ends", () => {
    expect(stepSelection(ids, null, 1)).toBe("a");
    expect(stepSelection(ids, null, -1)).toBe("e");
    expect(stepSelection(ids, "e", 1)).toBe("e");
    expect(stepSelection(ids, "c", -1)).toBe("b");
    expect(stepSelection([], "c", 1)).toBeNull();
  });
});

function audio(over: Partial<AudioInfo> = {}): AudioInfo {
  return {
    path: "x",
    name: "x",
    format: "WAV",
    codec: "PCM 24-bit",
    sampleRate: 48000,
    channels: 2,
    length: 48000,
    bitDepth: 24,
    fileSize: 10,
    ...over,
  };
}

function item(name: string, over: Partial<MediaItem> = {}): MediaItem {
  return { id: name, path: `C:\\Shows\\${name}`, name, status: "ready", info: audio(), error: null, ...over };
}

describe("column formatting", () => {
  it("formats durations", () => {
    expect(formatDuration(audio({ length: 0 }))).toBe("0:00.000");
    expect(formatDuration(audio({ length: 48000 * 75.399 }))).toBe("1:15.399");
    expect(formatDuration(audio({ length: 48000 * 4505.4 }))).toBe("1:15:05.400");
  });

  it("formats rate, channels and size", () => {
    expect(formatRate(44100)).toBe("44,100 Hz");
    expect(formatChannels(1)).toBe("Mono");
    expect(formatChannels(2)).toBe("Stereo");
    expect(formatChannels(6)).toBe("6 ch");
    expect(formatSize(512)).toBe("512 B");
    expect(formatSize(5 * 1024 * 1024)).toBe("5.0 MB");
  });

  it("sorts with unmeasured files last, numbers numerically", () => {
    const items = [
      item("ep10.wav", { info: audio({ length: 96000 }) }),
      item("ep2.wav", { info: audio({ length: 48000 }) }),
      item("new.wav", { status: "probing", info: null }),
    ];
    expect(sortItems(items, "name", false).map((i) => i.name)).toEqual(["ep2.wav", "ep10.wav", "new.wav"]);
    expect(sortItems(items, "duration", true).map((i) => i.name)).toEqual(["ep10.wav", "ep2.wav", "new.wav"]);
    expect(sortItems(items, null, false)).toEqual(items);
  });

  it("searches every word across path, format and codec", () => {
    const it = item("Interview.wav");
    expect(matches(it, "")).toBe(true);
    expect(matches(it, "shows interview")).toBe(true);
    expect(matches(it, "24-bit")).toBe(true);
    expect(matches(it, "mp3")).toBe(false);
  });
});
