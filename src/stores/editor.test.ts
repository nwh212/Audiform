// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { editor, parseSettings, restoreSettings, settingsOf } from "./editor";

describe("editor settings", () => {
  it("keeps defaults for missing or bad values", () => {
    const s = parseSettings({ snap: { on: false, ruler: "yes" }, autoCrossfade: 1, timeFormat: "samples" });
    expect(s.snap).toEqual({ on: false, clips: true, playhead: true, markers: true, ruler: true });
    expect(s.autoCrossfade).toBe(true);
    expect(s.timeFormat).toBe("samples");
    expect(parseSettings(null).snap.on).toBe(true);
  });

  it("restores saved settings into the editor", () => {
    restoreSettings({ snap: { clips: false }, autoCrossfade: false });
    const s = settingsOf(editor.current());
    expect(s.snap.clips).toBe(false);
    expect(s.autoCrossfade).toBe(false);
  });

  it("keeps the always-snap-to-zero choice, off by default", () => {
    expect(parseSettings({}).snapZero).toBe(false);
    expect(parseSettings({ snapZero: "yes" }).snapZero).toBe(false);
    restoreSettings({ snapZero: true });
    expect(settingsOf(editor.current()).snapZero).toBe(true);
    editor.setSnapZero(false);
    expect(editor.current().snapZero).toBe(false);
  });
});

describe("play mode", () => {
  it("toggles between loop, skip and normal, and survives a restart", () => {
    editor.togglePlayMode("loop");
    expect(editor.current().playMode).toBe("loop");
    editor.togglePlayMode("skip");
    expect(editor.current().playMode).toBe("skip");
    editor.togglePlayMode("skip");
    expect(editor.current().playMode).toBe("normal");
    expect(parseSettings({ playMode: "loop" }).playMode).toBe("loop");
    expect(parseSettings({ playMode: "sideways" }).playMode).toBe("normal");
  });
});

describe("marker pick", () => {
  it("replaces the pick, clears with an empty list, and leaves the clip pick alone", () => {
    editor.selectClips(["a"]);
    editor.selectMarkers(["m1", "m2"]);
    expect([...editor.current().selectedMarkers]).toEqual(["m1", "m2"]);
    expect(editor.current().selectedClips.has("a")).toBe(true);
    editor.selectMarkers(["m3"]);
    expect([...editor.current().selectedMarkers]).toEqual(["m3"]);
    editor.selectMarkers([]);
    expect(editor.current().selectedMarkers.size).toBe(0);
  });
});
