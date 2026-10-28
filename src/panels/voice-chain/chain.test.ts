// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { EffectInfo, EffectParamInfo } from "../../app/core";
import {
  addSlot,
  addSlotAt,
  EMPTY_CHAIN,
  layoutSlots,
  moveSlotToIndex,
  pluginMenuGroups,
  applyPreset,
  batchPresets,
  BUILTIN_PRESETS,
  chainOf,
  clampParam,
  formatParam,
  fromSlider,
  MAX_SLOTS,
  moveSlot,
  moveSlotTo,
  replaceSlot,
  newSlot,
  presetFrom,
  removeSlot,
  sanitizePresets,
  setParam,
  toSlider,
} from "./chain";

// The ids the core's catalog has (src-tauri/src/dsp/effects).
const KNOWN = new Set([
  "altaudio.parametric_eq",
  "altaudio.hum_cut",
  "altaudio.dynamics",
  "altaudio.limiter",
  "altaudio.echo",
  "altaudio.breath_gate",
  "altaudio.deesser",
  "altaudio.speech_leveler",
  "altaudio.gain",
  "altaudio.noise_reducer",
  "altaudio.noise_print",
  "altaudio.sound_eraser",
  "altaudio.hiss_reducer",
  "altaudio.room_reducer",
  "altaudio.click_repair",
  "altaudio.clip_repair",
  "altaudio.dc_offset",
  "altaudio.band_eq_10",
  "altaudio.band_eq_20",
  "altaudio.band_eq_30",
  "altaudio.notch",
  "altaudio.precision_filter",
  "altaudio.curve_eq",
  "altaudio.center_remover",
  "altaudio.center_extractor",
  "altaudio.pitch_shift",
  "altaudio.phase_rotate",
]);

const p = (over: Partial<EffectParamInfo>): EffectParamInfo => ({
  key: "k",
  name: "K",
  unit: "dB",
  min: -24,
  max: 24,
  default: 0,
  step: 0.1,
  log: false,
  toggle: false,
  ...over,
});
const gain: EffectInfo = {
  id: "altaudio.gain",
  name: "Gain",
  category: "Amplitude",
  description: "",
  params: [p({ key: "gain" })],
};

let n = 0;
const id = () => `s${n++}`;

describe("voice chain", () => {
  it("adds, moves, removes and caps at 16", () => {
    let c = chainOf(undefined);
    c = addSlot(c, newSlot(gain, "a"))!;
    c = addSlot(c, newSlot(gain, "b"))!;
    c = addSlot(c, newSlot(gain, "c"), 0)!;
    expect(c.slots.map((s) => s.id)).toEqual(["c", "a", "b"]);
    expect(moveSlot(c, "c", 1).slots.map((s) => s.id)).toEqual(["a", "c", "b"]);
    expect(moveSlot(c, "c", -1)).toBe(c);
    expect(removeSlot(c, "a").slots.map((s) => s.id)).toEqual(["c", "b"]);
    for (let i = c.slots.length; i < MAX_SLOTS; i++)
      c = addSlot(c, newSlot(gain, id()))!;
    expect(addSlot(c, newSlot(gain, "x"))).toBeNull();
  });

  it("snaps values and maps sliders, log ones too", () => {
    const freq = p({
      unit: "Hz",
      min: 20,
      max: 20_000,
      default: 1000,
      step: 1,
      log: true,
    });
    expect(toSlider(freq, 20)).toBe(0);
    expect(toSlider(freq, 20_000)).toBeCloseTo(1);
    expect(toSlider(freq, 632)).toBeCloseTo(0.5, 2);
    expect(fromSlider(freq, 0.5)).toBe(632);
    expect(clampParam(p({}), 0.30000000000000004)).toBe(0.3);
    expect(clampParam(p({}), 99)).toBe(24);
    expect(clampParam(p({}), Number.NaN)).toBe(0);
    let c = addSlot(chainOf(null), { ...newSlot(gain, "g"), preset: "Old" })!;
    c = setParam(c, "g", gain.params[0]!, -6.04);
    expect(c.slots[0]).toMatchObject({ params: { gain: -6 }, preset: null });
  });

  it("formats values for the controls", () => {
    expect(formatParam(p({}), -6)).toBe("-6.0 dB");
    expect(formatParam(p({ unit: ":1" }), 4)).toBe("4.0:1");
    expect(formatParam(p({ unit: "Hz", step: 1 }), 1200)).toBe("1.20 kHz");
    expect(formatParam(p({ unit: "Hz", step: 1 }), 80)).toBe("80 Hz");
    expect(formatParam(p({ toggle: true }), 1)).toBe("On");
    expect(
      formatParam(p({ choices: ["12 dB/oct", "24 dB/oct"], min: 0 }), 1),
    ).toBe("24 dB/oct");
  });

  it("ships presets that only use real effects, with unique names", () => {
    const names = BUILTIN_PRESETS.map((x) => x.name);
    expect(new Set(names).size).toBe(names.length);
    for (const pr of BUILTIN_PRESETS) {
      expect(pr.chain.slots.length).toBeGreaterThan(0);
      for (const s of pr.chain.slots)
        expect(KNOWN.has(s.effect), `${pr.name}: ${s.effect}`).toBe(true);
    }
    // The limiter default the project chose: -6 dB, with a -3 dB variant.
    const lim = (name: string) =>
      BUILTIN_PRESETS.find((x) => x.name === name)!.chain.slots.find(
        (s) => s.effect === "altaudio.limiter",
      )!.params.ceiling;
    expect(lim("Podcast Voice")).toBe(-6);
    expect(lim("Podcast Voice 3 -3 dB")).toBe(-3);
  });

  it("voice chain presets replace; one-effect presets add", () => {
    let c = addSlot(chainOf(null), newSlot(gain, "keep"))!;
    const hum = BUILTIN_PRESETS.find((x) => x.name === "Hum Cut 60 Hz (US)")!;
    const voice = BUILTIN_PRESETS.find((x) => x.name === "Podcast Voice")!;
    c = applyPreset(c, hum, KNOWN, id)!;
    expect(c.slots.map((s) => s.effect)).toEqual([
      "altaudio.gain",
      "altaudio.hum_cut",
    ]);
    expect(c.slots[1]!.preset).toBe("Hum Cut 60 Hz (US)");
    c = applyPreset(c, voice, KNOWN, id)!;
    expect(c.slots.some((s) => s.id === "keep")).toBe(false);
    // Unknown effects in a preset are skipped, never sent to the core.
    expect(
      applyPreset(c, voice, new Set(["altaudio.limiter"]), id)!.slots,
    ).toHaveLength(1);
  });

  it("keeps per-voice track chains out of Batch Convert", () => {
    const names = batchPresets(BUILTIN_PRESETS).map((p) => p.name);
    for (const n of [
      "Podcast Voice",
      "Podcast Voice 4 -3 dB",
      "Radio Announcer Voice",
      "Voice Over",
      "Remote Guest Cleanup",
    ])
      expect(names).not.toContain(n);
    for (const n of [
      "Voice Leveler and Noise Gate",
      "DeEss and Limit Male Voice Over",
      "Hard Limit to -0.1 dB",
      "Remove Vocals",
      "Remove Clicks",
    ])
      expect(names).toContain(n);
  });

  it("saves and reloads user presets safely", () => {
    const c = addSlot(chainOf(null), newSlot(gain, "g"))!;
    const saved = JSON.parse(
      JSON.stringify([presetFrom("Mine", { ...c, output_gain_db: -2 })]),
    );
    const back = sanitizePresets([
      ...saved,
      null,
      { name: "" },
      { name: "Bad", chain: { slots: "x" } },
    ]);
    expect(back).toHaveLength(1);
    expect(back[0]).toMatchObject({
      name: "Mine",
      chain: { output_gain_db: -2, slots: [{ effect: "altaudio.gain" }] },
    });
    expect(sanitizePresets("nope")).toEqual([]);
  });

  it("has character voice chains for the Audio FX chain only, with unique names", () => {
    const names = BUILTIN_PRESETS.map((p) => p.name);
    expect(new Set(names).size).toBe(names.length);
    const batch = batchPresets(BUILTIN_PRESETS).map((p) => p.name);
    for (const n of [
      "Godlike",
      "Demonic",
      "Evil Voice",
      "Ethereal",
      "Angelic",
    ]) {
      expect(names).toContain(n);
      expect(batch).not.toContain(n);
    }
  });

  it("moves a slot to an index and swaps one in place", () => {
    let c = chainOf(null);
    for (const k of ["a", "b", "c"]) c = addSlot(c, newSlot(gain, k))!;
    expect(moveSlotTo(c, "a", 2).slots.map((x) => x.id)).toEqual([
      "b",
      "c",
      "a",
    ]);
    expect(moveSlotTo(c, "c", 0).slots.map((x) => x.id)).toEqual([
      "c",
      "a",
      "b",
    ]);
    expect(moveSlotTo(c, "b", 1)).toBe(c);
    expect(moveSlotTo(c, "a", 99).slots.map((x) => x.id)).toEqual([
      "b",
      "c",
      "a",
    ]);
    const r = replaceSlot(c, "b", { ...newSlot(gain, "z") });
    expect(r.slots.map((x) => x.id)).toEqual(["a", "z", "c"]);
  });
});

describe("plug-in menu groups", () => {
  it("groups by type and maker, Other last", () => {
    const p = (name: string, vendor: string, kind: string) => ({ effect: `vst3:${name}`, name, vendor, kind });
    const g = pluginMenuGroups([p("Zeta Verb", "Acme", "Fx|Reverb"), p("Alpha Verb", "Bolt", "Fx|Reverb"), p("Comp", "Acme", "Fx|Dynamics"), p("Odd", "", "")]);
    expect(g.types.map(([k, l]) => [k, l.map((x) => x.name)])).toEqual([
      ["Dynamics", ["Comp"]],
      ["Reverb", ["Alpha Verb", "Zeta Verb"]],
      ["Other", ["Odd"]],
    ]);
    expect(g.makers.map(([k]) => k)).toEqual(["Acme", "Bolt", "Other"]);
  });
});

describe("numbered slots", () => {
  const s = (id: string, position?: number) => ({ id, effect: "x", enabled: true, params: {}, preset: null, ...(position === undefined ? {} : { position }) });
  const chain = (...slots: ReturnType<typeof s>[]) => ({ ...EMPTY_CHAIN, slots });
  const ids = (c: { slots: { id: string }[] } | null) => layoutSlots(c as never).map((x) => x?.id ?? "-").join("");

  it("shows effects in their slots, gaps and all", () => {
    expect(ids(chain(s("a"), s("b")))).toBe("ab" + "-".repeat(14));
    expect(ids(chain(s("a", 2), s("b"), s("c", 1)))).toBe("--abc" + "-".repeat(11));
    // Never past the end: the last effects squeeze in.
    expect(ids(chain(s("a", 15), s("b")))).toBe("-".repeat(14) + "ab");
  });

  it("adds an effect to any slot", () => {
    const c = addSlotAt(chain(s("a")), s("n"), 5)!;
    expect(ids(c)).toBe("a----n" + "-".repeat(10));
    expect(c.slots.map((x) => x.id)).toEqual(["a", "n"]);
    // Into an occupied slot: the one there moves down.
    const d = addSlotAt(c, s("m"), 0)!;
    expect(ids(d)).toBe("ma---n" + "-".repeat(10));
  });

  it("moves an effect to another slot, keeping the others in place", () => {
    const c = chain(s("a"), s("b"), s("c", 5));
    expect(ids(moveSlotToIndex(c, "a", 3))).toBe("-b-a-c" + "-".repeat(10));
    expect(moveSlotToIndex(c, "a", 3).slots.map((x) => x.id)).toEqual(["b", "a", "c"]);
    expect(ids(moveSlotToIndex(c, "c", 0))).toBe("cab" + "-".repeat(13));
    // Run order follows the slots.
    expect(moveSlotToIndex(c, "c", 0).slots.map((x) => x.id)).toEqual(["c", "a", "b"]);
  });

  it("moving an effect up or down swaps places and leaves gaps where they were", () => {
    const base = addSlotAt(addSlotAt(chain(), s("a"), 3)!, s("b"), 6)!;
    const down = moveSlot(base, "a", 1);
    const rows = layoutSlots(down);
    expect(rows[3]?.id).toBe("b");
    expect(rows[6]?.id).toBe("a");
  });

  it("never pushes an effect past the last slot", () => {
    const last = addSlotAt(chain(), s("a"), MAX_SLOTS - 1)!;
    const next = addSlotAt(last, s("b"), MAX_SLOTS - 1)!;
    const rows = layoutSlots(next);
    expect(rows[MAX_SLOTS - 2]?.id).toBe("b");
    expect(rows[MAX_SLOTS - 1]?.id).toBe("a");
  });
});

describe("instrument menu groups", () => {
  it("groups instruments by their own type, not by being instruments", () => {
    const p = (name: string, kind: string, vendor = "Maker") => ({ effect: `vst3:${name}`, name, vendor, kind });
    const { types, makers } = pluginMenuGroups([p("Keys", "Instrument|Piano"), p("Pad", "Instrument|Synth"), p("Kit", "Instrument", "Other Co")]);
    expect(types.map(([t]) => t)).toEqual(["Piano", "Synth", "Other"]);
    expect(makers.map(([m]) => m)).toEqual(["Maker", "Other Co"]);
  });

});
