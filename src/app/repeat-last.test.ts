// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { EffectChainDoc } from "../stores/session";
import { freshCopy, isPlugin, repeatName } from "./repeat-last";

const chain = (...effects: string[]): EffectChainDoc => ({
  enabled: true,
  input_gain_db: 0,
  output_gain_db: -1.5,
  mix: 0.8,
  slots: effects.map((effect, i) => ({ id: `s${i}`, effect, enabled: true, params: { gain: i + 1 }, preset: null, plugin_state: effect.startsWith("vst3:") ? "c3RhdGU=" : null, position: i + 4 })),
});

describe("Repeat Last Command", () => {
  it("copies the effects with their settings, under new ids", () => {
    let n = 0;
    const kept = chain("altaudio.parametric_eq", "vst3:abc");
    const copy = freshCopy(kept, () => `new${n++}`);
    expect(copy.slots.map((s) => s.id)).toEqual(["new0", "new1"]);
    expect(copy.slots.map((s) => s.params)).toEqual([{ gain: 1 }, { gain: 2 }]);
    expect(copy.slots[1]!.plugin_state).toBe("c3RhdGU=");
    expect(copy.slots.every((s) => s.position === undefined)).toBe(true);
    expect([copy.output_gain_db, copy.mix]).toEqual([-1.5, 0.8]);
    // The kept copy is untouched, so the next repeat is the same again.
    copy.slots[0]!.params.gain = 9;
    expect(kept.slots[0]!.params.gain).toBe(1);
    expect(kept.slots.map((s) => s.id)).toEqual(["s0", "s1"]);
  });

  it("names one effect, or the chain", () => {
    const names = new Map([["altaudio.parametric_eq", "Parametric EQ"]]);
    expect(repeatName(chain("altaudio.parametric_eq"), names)).toBe("Parametric EQ");
    expect(repeatName(chain("vst3:gone"), names)).toBe("Effect");
    expect(repeatName(chain("altaudio.parametric_eq", "altaudio.gain"), names)).toBe("Audio FX");
  });

  it("knows a plug-in slot", () => {
    expect(chain("vst3:abc", "altaudio.gain").slots.map(isPlugin)).toEqual([true, false]);
  });
});
