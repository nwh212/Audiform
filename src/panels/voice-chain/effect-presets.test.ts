// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { deletePreset, presetChange, sanitizeEffectPresets, savePreset } from "./effect-presets";

const slot = { id: "s", effect: "altaudio.gain", enabled: true, params: { gain_db: 3 }, preset: null };

describe("effect presets", () => {
  it("saves, replaces and deletes by name", () => {
    let all = savePreset({}, slot, "Loud");
    all = savePreset(all, { ...slot, params: { gain_db: 6 } }, "Loud");
    expect(all["altaudio.gain"]).toEqual([{ name: "Loud", params: { gain_db: 6 } }]);
    expect(deletePreset(all, "altaudio.gain", "Loud")).toEqual({});
  });
  it("applies over the defaults and names the slot", () => {
    expect(presetChange({ name: "Loud", params: { gain_db: 6 } }, { gain_db: 0, trim: 1 })).toEqual({ params: { gain_db: 6, trim: 1 }, data: null, preset: "Loud" });
  });
  it("drops what it cannot read", () => {
    expect(sanitizeEffectPresets({ a: [{ name: "x", params: { k: 1, bad: "no" } }, { name: 3 }], b: "no" })).toEqual({ a: [{ name: "x", params: { k: 1 } }] });
    expect(sanitizeEffectPresets(null)).toEqual({});
  });
});
