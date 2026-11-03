// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Audio FX edits as plain functions: each returns a new chain, so the
 * panel sends one undoable `set_effects` per change. Also the built-in
 * presets and the slider maths.
 */

import type { EffectInfo, EffectParamInfo } from "../../app/core";
import type { EffectChainDoc, EffectSlotDoc } from "../../stores/session";

export const MAX_SLOTS = 16;

export const EMPTY_CHAIN: EffectChainDoc = {
  enabled: true,
  input_gain_db: 0,
  output_gain_db: 0,
  mix: 1,
  slots: [],
};

export function chainOf(c: EffectChainDoc | undefined | null): EffectChainDoc {
  return c
    ? { ...EMPTY_CHAIN, ...c, slots: c.slots ?? [] }
    : { ...EMPTY_CHAIN, slots: [] };
}

/** A new slot with every parameter at its default. */
export function newSlot(info: EffectInfo, id: string): EffectSlotDoc {
  return {
    id,
    effect: info.id,
    enabled: true,
    params: Object.fromEntries(info.params.map((p) => [p.key, p.default])),
    preset: null,
  };
}

/** Adds an effect at `index` (the end when missing). Null when the chain is full. */
export function addSlot(
  chain: EffectChainDoc,
  slot: EffectSlotDoc,
  index?: number,
): EffectChainDoc | null {
  if (chain.slots.length >= MAX_SLOTS) return null;
  const slots = [...chain.slots];
  slots.splice(
    index === undefined
      ? slots.length
      : Math.max(0, Math.min(index, slots.length)),
    0,
    slot,
  );
  return { ...chain, slots };
}

/** Puts an effect in the first empty numbered slot. Null when the chain is full. */
export function addSlotFirstFree(chain: EffectChainDoc, slot: EffectSlotDoc): EffectChainDoc | null {
  const free = layoutSlots(chain).findIndex((s) => s === null);
  return free < 0 ? null : addSlotAt(chain, slot, free);
}

/**
 * The chain as its numbered slots: each effect in the slot it asked for
 * (its `position`), or the one after the effect before it. Effects run
 * in list order, so a slot never comes before an earlier effect's.
 */
export function layoutSlots(chain: EffectChainDoc): (EffectSlotDoc | null)[] {
  const out: (EffectSlotDoc | null)[] = Array.from({ length: MAX_SLOTS }, () => null);
  const n = chain.slots.length;
  let next = 0;
  chain.slots.forEach((s, k) => {
    // Leave room for the effects still to come.
    const latest = MAX_SLOTS - (n - k);
    const at = Math.min(latest, Math.max(next, s.position ?? next));
    out[at] = s;
    next = at + 1;
  });
  return out;
}

/** How many effects show before numbered slot `at`, leaving out `skip`. */
function countBefore(chain: EffectChainDoc, at: number, skip?: string): number {
  return layoutSlots(chain)
    .slice(0, at)
    .filter((s) => s && s.id !== skip).length;
}

/**
 * Puts an effect in numbered slot `at`. An effect already there moves
 * down one. Null when the chain is full.
 */
export function addSlotAt(chain: EffectChainDoc, slot: EffectSlotDoc, at: number): EffectChainDoc | null {
  if (chain.slots.length >= MAX_SLOTS) return null;
  const to = Math.max(0, Math.min(MAX_SLOTS - 1, at));
  const slots = pinned(chain);
  slots.splice(countBefore(chain, to), 0, { ...slot, position: to });
  return { ...chain, slots: bumped(slots) };
}

/** Moves an effect to numbered slot `at`; an effect already there moves down one. */
export function moveSlotToIndex(chain: EffectChainDoc, id: string, at: number): EffectChainDoc {
  const s = chain.slots.find((x) => x.id === id);
  if (!s) return chain;
  const to = Math.max(0, Math.min(MAX_SLOTS - 1, at));
  const before = countBefore(chain, to, id);
  const slots = pinned(chain).filter((x) => x.id !== id);
  slots.splice(before, 0, { ...s, position: to });
  return { ...chain, slots: bumped(slots) };
}

/** Every effect pinned to the slot it shows in now, so moving one leaves the rest in place. */
function pinned(chain: EffectChainDoc): EffectSlotDoc[] {
  const where = new Map(layoutSlots(chain).flatMap((s, i) => (s ? [[s.id, i] as const] : [])));
  return chain.slots.map((s) => ({ ...s, position: where.get(s.id) ?? s.position ?? null }));
}

/** A reordered list takes the numbered places the chain showed, in order. */
function keepPlaces(chain: EffectChainDoc, slots: EffectSlotDoc[]): EffectSlotDoc[] {
  const places = layoutSlots(chain).flatMap((s, i) => (s ? [i] : []));
  return slots.map((s, k) => ({ ...s, position: places[k] ?? null }));
}

/** Pushes effects down where one now sits on or above the one before; none goes past the last slot. */
function bumped(slots: EffectSlotDoc[]): EffectSlotDoc[] {
  let next = 0;
  const out = slots.map((s) => {
    const at = Math.max(next, s.position ?? next);
    next = at + 1;
    return at === s.position ? s : { ...s, position: at };
  });
  for (let k = out.length - 1, limit = MAX_SLOTS - 1; k >= 0; k--, limit--) {
    const s = out[k]!;
    if ((s.position ?? 0) > limit) out[k] = { ...s, position: limit };
  }
  return out;
}

export function removeSlot(chain: EffectChainDoc, id: string): EffectChainDoc {
  return { ...chain, slots: chain.slots.filter((s) => s.id !== id) };
}

/** Moves a slot up (-1) or down (+1). Unchanged at either end. */
export function moveSlot(
  chain: EffectChainDoc,
  id: string,
  by: number,
): EffectChainDoc {
  const i = chain.slots.findIndex((s) => s.id === id);
  const j = i + by;
  if (i < 0 || j < 0 || j >= chain.slots.length) return chain;
  const slots = [...chain.slots];
  const [s] = slots.splice(i, 1);
  slots.splice(j, 0, s!);
  return { ...chain, slots: keepPlaces(chain, slots) };
}

/** Moves a slot to `to` (an index in the current list). Unchanged when it is already there. */
export function moveSlotTo(
  chain: EffectChainDoc,
  id: string,
  to: number,
): EffectChainDoc {
  const i = chain.slots.findIndex((s) => s.id === id);
  const j = Math.max(0, Math.min(to, chain.slots.length - 1));
  if (i < 0 || i === j) return chain;
  const slots = [...chain.slots];
  const [s] = slots.splice(i, 1);
  slots.splice(j, 0, s!);
  return { ...chain, slots: keepPlaces(chain, slots) };
}

/** Puts `slot` where the slot `id` is, keeping its place in the list. */
export function replaceSlot(
  chain: EffectChainDoc,
  id: string,
  slot: EffectSlotDoc,
): EffectChainDoc {
  return { ...chain, slots: chain.slots.map((s) => (s.id === id ? slot : s)) };
}

export function setSlot(
  chain: EffectChainDoc,
  id: string,
  change: Partial<EffectSlotDoc>,
): EffectChainDoc {
  return {
    ...chain,
    slots: chain.slots.map((s) => (s.id === id ? { ...s, ...change } : s)),
  };
}

/** Sets one parameter, snapped to its step and range. Clears the preset name. */
export function setParam(
  chain: EffectChainDoc,
  id: string,
  p: EffectParamInfo,
  value: number,
): EffectChainDoc {
  const v = clampParam(p, value);
  return {
    ...chain,
    slots: chain.slots.map((s) =>
      s.id === id
        ? { ...s, params: { ...s.params, [p.key]: v }, preset: null }
        : s,
    ),
  };
}

export function clampParam(p: EffectParamInfo, value: number): number {
  if (!Number.isFinite(value)) return p.default;
  const snapped = p.step > 0 ? Math.round(value / p.step) * p.step : value;
  // Trim float noise from the step maths (0.30000000000000004).
  return Math.min(p.max, Math.max(p.min, Number(snapped.toFixed(6))));
}

export function paramValue(slot: EffectSlotDoc, p: EffectParamInfo): number {
  const v = slot.params[p.key];
  return typeof v === "number" && Number.isFinite(v)
    ? clampParam(p, v)
    : p.default;
}

/** Slider position 0..1 for a value, logarithmic where the control asks for it. */
export function toSlider(p: EffectParamInfo, v: number): number {
  if (p.max <= p.min) return 0;
  if (p.log) return Math.log(v / p.min) / Math.log(p.max / p.min);
  return (v - p.min) / (p.max - p.min);
}

export function fromSlider(p: EffectParamInfo, t: number): number {
  const x = Math.min(1, Math.max(0, t));
  const v = p.log
    ? p.min * Math.pow(p.max / p.min, x)
    : p.min + x * (p.max - p.min);
  return clampParam(p, v);
}

/** A value as the control shows it: "-6.0 dB", "4.0:1", "1.2 kHz", "On". */
export function formatParam(p: EffectParamInfo, v: number): string {
  if (p.toggle) return v >= 0.5 ? "On" : "Off";
  if (p.choices?.length) return p.choices[Math.round(v - p.min)] ?? String(v);
  if (p.unit === "Hz" && v >= 1000)
    return `${(v / 1000).toFixed(v >= 10_000 ? 1 : 2)} kHz`;
  const digits = p.step >= 1 ? 0 : p.step >= 0.1 ? 1 : 2;
  const text = v.toFixed(digits);
  if (!p.unit) return text;
  return p.unit.startsWith(":") ? `${text}${p.unit}` : `${text} ${p.unit}`;
}

/** Badge colors by group, so the slot list reads at a glance. */
export const CATEGORY_COLORS: Record<string, string> = {
  "Filter and EQ": "#4f9fd8",
  Dynamics: "#d8964f",
  Amplitude: "#8bc06a",
  Restoration: "#b07ad8",
  "Time and Pitch": "#d86f9b",
  Stereo: "#4fc8b8",
  "Delay and Reverb": "#9d7cf0",
  Modulation: "#5fb0e8",
  Vocal: "#ec6a92",
};

// --- Presets ---

export interface ChainPreset {
  name: string;
  /** "Voice chains" replace the chain; the single-effect groups add one slot. */
  group: string;
  /** A full per-voice chain for a host's track: offered in the Audio FX chain only, not in Batch Convert. */
  trackOnly?: boolean;
  chain: Omit<EffectChainDoc, "slots"> & {
    /** A plug-in's own settings ride along in `plugin_state`; `enabled` false keeps it bypassed. */
    slots: { effect: string; params: Record<string, number>; plugin_state?: string | null; enabled?: boolean }[];
  };
}

/** True when choosing the preset adds one effect instead of replacing the chain. */
export function addsOne(p: ChainPreset): boolean {
  return p.chain.slots.length === 1 && p.group !== "Voice chains";
}

const base = { enabled: true, input_gain_db: 0, output_gain_db: 0, mix: 1 };
const fx = (effect: string, params: Record<string, number> = {}) => ({
  effect: `altaudio.${effect}`,
  params,
});
const chainP = (
  name: string,
  group: string,
  slots: ReturnType<typeof fx>[],
  extra: Partial<EffectChainDoc> = {},
): ChainPreset => ({
  name,
  group,
  chain: { ...base, ...extra, slots },
});
const trackOnly = (p: ChainPreset): ChainPreset => ({ ...p, trackOnly: true });

/** Presets Batch Convert offers: everything but the per-voice track chains. */
export function batchPresets(list: readonly ChainPreset[]): ChainPreset[] {
  return list.filter((p) => !p.trackOnly);
}

// Our own starting points, tuned by ear on spoken word. The order follows
// the podcast chain: level, compress, shape, de-ess, limit, then gate.
const voiceEq = fx("parametric_eq", {
  hp_on: 1,
  hp_freq: 80,
  hp_slope: 1,
  b1_freq: 300,
  b1_gain: -2,
  b1_q: 1.2,
  b3_freq: 4000,
  b3_gain: 2,
  b3_q: 0.9,
});
const leveler = fx("speech_leveler", {
  target: -20,
  max_gain: 8,
  max_cut: 8,
  speed: 2000,
  floor: -50,
});
const comp = (threshold: number, ratio: number, makeup: number) =>
  fx("dynamics", {
    threshold,
    ratio,
    attack: 8,
    release: 160,
    knee: 6,
    makeup,
  });
const limit = (ceiling: number) =>
  fx("limiter", { ceiling, lookahead: 5, release: 100 });
const cleanNoise = (reduction: number) =>
  fx("noise_reducer", { reduction, strength: 2, learn: 2, smoothing: 50 });
const gate = (threshold: number) =>
  fx("breath_gate", {
    threshold,
    attack: 2,
    hold: 50,
    release: 200,
    range: -30,
  });

export const BUILTIN_PRESETS: ChainPreset[] = [
  trackOnly(
    chainP("Podcast Voice", "Voice chains", [
      cleanNoise(10),
      leveler,
      comp(-22, 3, 4),
      voiceEq,
      fx("deesser", { freq: 6500, threshold: -32, reduction: 6 }),
      limit(-6),
    ]),
  ),
  trackOnly(
    chainP("Podcast Voice 2", "Voice chains", [
      cleanNoise(12),
      leveler,
      comp(-24, 4, 6),
      voiceEq,
      fx("deesser", { freq: 6500, threshold: -30, reduction: 8 }),
      limit(-6),
      gate(-45),
    ]),
  ),
  trackOnly(
    chainP("Podcast Voice 3", "Voice chains", [
      comp(-20, 2.5, 3),
      voiceEq,
      limit(-6),
    ]),
  ),
  trackOnly(
    chainP("Podcast Voice 3 -3 dB", "Voice chains", [
      comp(-20, 2.5, 3),
      voiceEq,
      limit(-3),
    ]),
  ),
  trackOnly(
    chainP("Podcast Voice 4 -3 dB", "Voice chains", [
      cleanNoise(10),
      leveler,
      comp(-26, 3.5, 6),
      voiceEq,
      fx("deesser", { freq: 6000, threshold: -32, reduction: 6 }),
      limit(-3),
    ]),
  ),
  // Character voices: the same chain order as a clean voice, plus a pitch move
  // and a tone. Starting points tuned by ear. A reverb stage joins them once
  // we have a reverb effect.
  trackOnly(
    chainP("Godlike", "Voice chains", [
      cleanNoise(10),
      leveler,
      fx("parametric_eq", {
        hp_on: 1,
        hp_freq: 60,
        hp_slope: 1,
        ls_freq: 140,
        ls_gain: 5,
        b2_freq: 2500,
        b2_gain: 2,
        b2_q: 0.9,
      }),
      comp(-24, 3, 5),
      fx("pitch_shift", { semitones: -3, cents: 0 }),
      limit(-3),
    ]),
  ),
  trackOnly(
    chainP("Demonic", "Voice chains", [
      cleanNoise(10),
      leveler,
      fx("parametric_eq", {
        hp_on: 1,
        hp_freq: 70,
        hp_slope: 1,
        ls_freq: 120,
        ls_gain: 6,
        b2_freq: 1200,
        b2_gain: -3,
        b2_q: 1,
        hs_freq: 6000,
        hs_gain: -3,
      }),
      comp(-26, 4, 6),
      fx("pitch_shift", { semitones: -8, cents: 0 }),
      limit(-3),
    ]),
  ),
  trackOnly(
    chainP("Evil Voice", "Voice chains", [
      cleanNoise(10),
      leveler,
      fx("parametric_eq", {
        hp_on: 1,
        hp_freq: 70,
        hp_slope: 1,
        ls_freq: 150,
        ls_gain: 4,
        b3_freq: 3200,
        b3_gain: 3,
        b3_q: 1.2,
      }),
      comp(-26, 4, 6),
      fx("pitch_shift", { semitones: -5, cents: 0 }),
      limit(-3),
    ]),
  ),
  trackOnly(
    chainP("Ethereal", "Voice chains", [
      cleanNoise(10),
      leveler,
      fx("parametric_eq", {
        hp_on: 1,
        hp_freq: 120,
        hp_slope: 1,
        b2_freq: 400,
        b2_gain: -2,
        b2_q: 1,
        hs_freq: 9000,
        hs_gain: 4,
      }),
      fx("pitch_shift", { semitones: 2, cents: 0 }),
      limit(-3),
    ]),
  ),
  trackOnly(
    chainP("Angelic", "Voice chains", [
      cleanNoise(10),
      leveler,
      fx("parametric_eq", {
        hp_on: 1,
        hp_freq: 100,
        hp_slope: 1,
        b3_freq: 4500,
        b3_gain: 3,
        b3_q: 0.9,
        hs_freq: 10_000,
        hs_gain: 3,
      }),
      fx("pitch_shift", { semitones: 3, cents: 0 }),
      limit(-3),
    ]),
  ),
  chainP("Voice Leveler and Noise Gate", "Voice chains", [leveler, gate(-40)]),
  chainP("Clean Up and Level Voice Over", "Voice chains", [
    cleanNoise(8),
    fx("click_repair", { sensitivity: 40, width: 1 }),
    fx("parametric_eq", { hp_on: 1, hp_freq: 70, hp_slope: 1 }),
    leveler,
    fx("deesser", { freq: 6500, threshold: -30, reduction: 6 }),
    limit(-3),
  ]),
  trackOnly(
    chainP("Radio Announcer Voice", "Voice chains", [
      comp(-28, 6, 9),
      fx("parametric_eq", {
        hp_on: 1,
        hp_freq: 60,
        ls_freq: 120,
        ls_gain: 4,
        b2_freq: 500,
        b2_gain: -2,
        b3_freq: 3500,
        b3_gain: 3,
        hs_freq: 10_000,
        hs_gain: 2,
      }),
      limit(-1),
    ]),
  ),
  trackOnly(
    chainP("Voice Over", "Voice chains", [
      fx("parametric_eq", { hp_on: 1, hp_freq: 70, hp_slope: 1 }),
      comp(-20, 2.5, 3),
      fx("deesser", { freq: 6500, threshold: -30, reduction: 6 }),
      limit(-3),
    ]),
  ),
  chainP("DeEss and Limit Male Voice Over", "Voice chains", [
    fx("deesser", { freq: 5500, threshold: -30, reduction: 8 }),
    limit(-3),
  ]),
  chainP("DeEss and Limit Female Voice Over", "Voice chains", [
    fx("deesser", { freq: 7500, threshold: -30, reduction: 8 }),
    limit(-3),
  ]),
  trackOnly(
    chainP("Remote Guest Cleanup", "Voice chains", [
      cleanNoise(18),
      fx("room_reducer", { amount: 50, decay: 0.6, reduction: 12 }),
      leveler,
      comp(-24, 4, 6),
      fx("parametric_eq", {
        hp_on: 1,
        hp_freq: 120,
        hp_slope: 2,
        b3_freq: 3000,
        b3_gain: 2,
      }),
      limit(-6),
      gate(-30),
    ]),
  ),
  chainP("Breath Gate", "Breath gates", [gate(-40)]),
  chainP("Breath Gate - Low", "Breath gates", [gate(-14)]),
  chainP("Breath Gate - Medium", "Breath gates", [gate(-20)]),
  chainP("Hard Limit to -0.1 dB", "Processing", [
    fx("limiter", {
      ceiling: -0.1,
      input_boost: 0,
      lookahead: 5,
      release: 100,
    }),
  ]),
  chainP("Voice-Over Compressor", "Processing", [
    fx("dynamics", {
      threshold: -24,
      ratio: 4,
      attack: 5,
      release: 120,
      knee: 6,
      makeup: 6,
    }),
  ]),
  chainP("Raise Pitch", "Processing", [
    fx("pitch_shift", { semitones: 2, cents: 0 }),
  ]),
  chainP("Lower Pitch", "Processing", [
    fx("pitch_shift", { semitones: -2, cents: 0 }),
  ]),
  chainP("Slapback Echo", "Delay", [
    fx("echo", { time: 110, feedback: 8, tone: 5000, wet: 28, pingpong: 0 }),
  ]),
  chainP("Podcast Room Echo", "Delay", [
    fx("echo", { time: 240, feedback: 25, tone: 4500, wet: 18, pingpong: 0 }),
  ]),
  chainP("Ping-Pong Echo", "Delay", [
    fx("echo", { time: 375, feedback: 40, tone: 6000, wet: 30, pingpong: 1 }),
  ]),
  chainP("Long Dark Echo", "Delay", [
    fx("echo", { time: 700, feedback: 55, tone: 2800, wet: 35, pingpong: 0 }),
  ]),
  chainP("Remove Vocals", "Processing", [
    fx("center_extractor", { range: 4, center_freq: 1342, width: 6.3, center: -48 }),
  ]),
  chainP("Remove Clicks", "Repair", [
    fx("click_repair", { sensitivity: 50, width: 1 }),
  ]),
  chainP("Repair Clipping", "Repair", [
    fx("clip_repair", { level: -0.5, longest: 3, output: -3 }),
  ]),
  chainP("Reduce Hiss", "Repair", [fx("hiss_reducer", {})]),
  chainP("Reduce Room Echo", "Repair", [fx("room_reducer", {})]),
  chainP("Fix DC Offset", "Repair", [fx("dc_offset", {})]),
  chainP("Hum Cut 60 Hz (US)", "EQ", [
    fx("hum_cut", { mains: 0, harmonics: 4, q: 25 }),
  ]),
  chainP("Hum Cut 50 Hz (Europe)", "EQ", [
    fx("hum_cut", { mains: 1, harmonics: 4, q: 25 }),
  ]),
  chainP("Rumble Cut", "EQ", [
    fx("parametric_eq", { hp_on: 1, hp_freq: 60, hp_slope: 1 }),
  ]),
  chainP("Subsonic Cut", "EQ", [
    fx("parametric_eq", { hp_on: 1, hp_freq: 25, hp_slope: 2 }),
  ]),
  // Instrument and character EQs. Bands the preset does not use are off;
  // Q is the band's width (lower is wider).
  chainP("Acoustic Strum", "EQ", [
    fx("parametric_eq", {
      ls_freq: 40,
      ls_gain: -6,
      b1_on: 0,
      b2_freq: 200,
      b2_gain: -3,
      b2_q: 1,
      b3_on: 0,
      b4_freq: 2000,
      b4_gain: -2,
      b4_q: 1,
      b5_freq: 5000,
      b5_gain: 2,
      b5_q: 1,
      hs_freq: 15000,
      hs_gain: 2,
    }),
  ]),
  chainP("Fat Snare", "EQ", [
    fx("parametric_eq", {
      ls_freq: 20,
      ls_gain: -20,
      b1_on: 0,
      b2_freq: 271,
      b2_gain: 8,
      b2_q: 4,
      b3_on: 0,
      b4_freq: 5705,
      b4_gain: -10,
      b4_q: 1,
      b5_on: 0,
      hs_freq: 20000,
      hs_gain: -10,
    }),
  ]),
  chainP("Kick Drum", "EQ", [
    fx("parametric_eq", {
      ls_freq: 20,
      ls_gain: -9,
      b1_freq: 80,
      b1_gain: 3,
      b1_q: 2,
      b2_freq: 120,
      b2_gain: -3,
      b2_q: 4,
      b3_on: 0,
      b4_freq: 2474,
      b4_gain: 4,
      b4_q: 2,
      b5_on: 0,
      hs_freq: 18000,
      hs_gain: -20,
    }),
  ]),
  chainP("Crunch Guitar", "EQ", [
    fx("parametric_eq", {
      ls_freq: 20,
      ls_gain: -15,
      b1_freq: 120,
      b1_gain: -3,
      b1_q: 1,
      b2_freq: 454,
      b2_gain: 4,
      b2_q: 4,
      b3_on: 0,
      b4_freq: 2938,
      b4_gain: -3,
      b4_q: 1,
      b5_on: 0,
      hs_freq: 18000,
      hs_gain: -15,
    }),
  ]),
  chainP("Vintage Radio", "EQ", [
    fx("parametric_eq", {
      gain: -3,
      ls_freq: 42,
      ls_gain: -5,
      b1_freq: 80,
      b1_gain: -24, // as deep as a band goes; the source curve dips a little further
      b1_q: 4,
      b2_freq: 200,
      b2_gain: -13,
      b2_q: 4,
      b3_freq: 672,
      b3_gain: 11,
      b3_q: 4,
      b4_freq: 2032,
      b4_gain: -12,
      b4_q: 4,
      b5_on: 0,
      hs_freq: 14000,
      hs_gain: -22,
    }),
  ]),
  chainP("Vocal Lift", "EQ", [
    fx("parametric_eq", {
      hp_on: 1,
      hp_freq: 80,
      hp_slope: 1,
      ls_freq: 110,
      ls_gain: 6,
      b1_on: 0,
      b2_on: 0,
      b3_freq: 291,
      b3_gain: -3,
      b3_q: 1.5,
      b4_freq: 3200,
      b4_gain: 0,
      b4_q: 2,
      b5_freq: 12800,
      b5_gain: 0,
      b5_q: 2,
      hs_freq: 15064,
      hs_gain: 10.1,
    }),
  ]),
  chainP("Flat", "EQ", [
    fx("parametric_eq", {
      b1_on: 0,
      b2_on: 0,
      b3_on: 0,
      b4_on: 0,
      b5_on: 0,
    }),
  ]),
  chainP("Smile Curve", "EQ", [
    fx("band_eq_10", { g01: 4, g02: 3, g03: 1, g04: 0, g05: -1, g06: -1, g07: 0, g08: 1, g09: 3, g10: 4 }),
  ]),
  chainP("Bass Boost", "EQ", [
    fx("band_eq_10", { g01: 5, g02: 4, g03: 2, g04: 1 }),
  ]),
  chainP("Treble Lift", "EQ", [
    fx("band_eq_10", { g07: 1, g08: 2, g09: 3, g10: 4 }),
  ]),
  chainP("Mud Cut", "EQ", [
    fx("band_eq_10", { g03: -3, g04: -3, g05: -1 }),
  ]),
  chainP("Low-Pass 8 kHz", "EQ", [
    fx("precision_filter", { design: 0, mode: 0, order: 1, freq: 8000 }),
  ]),
  chainP("High-Pass 100 Hz", "EQ", [
    fx("precision_filter", { design: 0, mode: 1, order: 1, freq: 100 }),
  ]),
  chainP("Band-Pass Voice", "EQ", [
    fx("precision_filter", { design: 0, mode: 2, order: 1, freq: 200, freq2: 5000 }),
  ]),
  chainP("Steep Low-Pass 16 kHz", "EQ", [
    fx("precision_filter", { design: 3, mode: 0, order: 3, freq: 16000, ripple: 0.3, stop: 80 }),
  ]),
  chainP("Notch 60 Hz and Harmonics", "EQ", [
    fx("notch", {
      n1_freq: 60,
      n1_depth: -40,
      n2_on: 1,
      n2_freq: 120,
      n2_depth: -30,
      n3_on: 1,
      n3_freq: 180,
      n3_depth: -24,
      n4_on: 1,
      n4_freq: 240,
      n4_depth: -18,
      width: 40,
    }),
  ]),
  chainP("Notch 50 Hz and Harmonics", "EQ", [
    fx("notch", {
      n1_freq: 50,
      n1_depth: -40,
      n2_on: 1,
      n2_freq: 100,
      n2_depth: -30,
      n3_on: 1,
      n3_freq: 150,
      n3_depth: -24,
      n4_on: 1,
      n4_freq: 200,
      n4_depth: -18,
      width: 40,
    }),
  ]),
  chainP("Whistle Cut", "EQ", [
    fx("notch", { n1_freq: 3000, n1_depth: -36, width: 30 }),
  ]),
  // Dynamics. Attack and release in milliseconds, ratio as n:1.
  chainP("Gentle Vocal Compressor", "Dynamics", [
    fx("dynamics", { threshold: -18, ratio: 2.5, attack: 10, release: 120, knee: 8, makeup: 3 }),
  ]),
  chainP("Tight Vocal Compressor", "Dynamics", [
    fx("dynamics", { threshold: -24, ratio: 4, attack: 5, release: 80, knee: 4, makeup: 6 }),
  ]),
  chainP("Drum Smash", "Dynamics", [
    fx("dynamics", { threshold: -30, ratio: 10, attack: 1, release: 60, knee: 0, makeup: 8 }),
  ]),
  chainP("Bass Glue", "Dynamics", [
    fx("dynamics", { threshold: -20, ratio: 4, attack: 30, release: 200, knee: 6, makeup: 4 }),
  ]),
  chainP("Acoustic Guitar Leveling", "Dynamics", [
    fx("dynamics", { threshold: -22, ratio: 3, attack: 15, release: 250, knee: 10, makeup: 3 }),
  ]),
  chainP("Mix Bus Glue", "Dynamics", [
    fx("dynamics", { threshold: -12, ratio: 1.5, attack: 30, release: 300, knee: 12, makeup: 1 }),
  ]),
  chainP("Limit to -1 dBTP", "Dynamics", [
    fx("limiter", { detect: 1, ceiling: -1, input_boost: 0, lookahead: 5, release: 100 }),
  ]),
  chainP("Loud Podcast Master", "Dynamics", [
    fx("limiter", { detect: 1, ceiling: -1, input_boost: 6, lookahead: 5, release: 150 }),
  ]),
  chainP("Transparent Safety Limiter", "Dynamics", [
    fx("limiter", { detect: 1, ceiling: -0.3, input_boost: 0, lookahead: 10, release: 300 }),
  ]),
  chainP("Gate - Tight Drums", "Dynamics", [
    fx("breath_gate", { threshold: -30, attack: 0.5, hold: 20, release: 60, range: -80 }),
  ]),
  chainP("Gate - Noisy Guitar Amp", "Dynamics", [
    fx("breath_gate", { threshold: -45, attack: 1, hold: 100, release: 300, range: -60 }),
  ]),
  chainP("Opto - Smooth Vocal", "Dynamics", [
    fx("opto_leveler", { peak: 45, gain: 50, mode: 0, emphasis: 0, tube: 30 }),
  ]),
  chainP("Opto - Bass", "Dynamics", [
    fx("opto_leveler", { peak: 60, gain: 55, mode: 0, emphasis: 20, tube: 40 }),
  ]),
  chainP("Opto - Mix Bus Limit", "Dynamics", [
    fx("opto_leveler", { peak: 30, gain: 40, mode: 1, emphasis: 0, tube: 15 }),
  ]),
  // Reverb. Decay in milliseconds, wet as a percentage of the dry level.
  chainP("Small Room", "Reverb", [
    fx("reverb", { kind: 0, size: 25, decay: 600, predelay: 5, early: 60, damping: 55, high_cut: 8000, wet: 18 }),
  ]),
  chainP("Vocal Plate", "Reverb", [
    fx("reverb", { kind: 2, size: 55, decay: 1800, predelay: 30, early: 30, damping: 45, diffusion: 85, high_cut: 10000, low_cut: 150, wet: 22 }),
  ]),
  chainP("Concert Hall", "Reverb", [
    fx("reverb", { kind: 1, size: 85, decay: 3200, predelay: 40, early: 35, damping: 50, diffusion: 75, high_cut: 7000, low_cut: 100, wet: 25 }),
  ]),
  chainP("Cathedral", "Reverb", [
    fx("reverb", { kind: 1, size: 100, decay: 7000, predelay: 80, early: 20, damping: 60, diffusion: 80, modulation: 40, high_cut: 6000, low_cut: 120, wet: 30 }),
  ]),
  chainP("Guitar Spring", "Reverb", [
    fx("reverb", { kind: 3, size: 40, decay: 2200, predelay: 0, early: 50, damping: 40, high_cut: 5000, low_cut: 200, wet: 28 }),
  ]),
  chainP("Tape Echo Chamber", "Reverb", [
    fx("reverb", { kind: 4, size: 60, decay: 2500, predelay: 60, early: 40, damping: 65, modulation: 50, high_cut: 5500, low_cut: 150, wet: 25 }),
  ]),
  chainP("Drum Room", "Reverb", [
    fx("reverb", { kind: 0, size: 45, decay: 900, predelay: 10, early: 70, damping: 35, diffusion: 60, high_cut: 9000, low_cut: 100, wet: 20 }),
  ]),
  chainP("Ambience Only", "Reverb", [
    fx("reverb", { kind: 0, size: 15, decay: 300, predelay: 0, early: 90, damping: 60, high_cut: 7000, wet: 12 }),
  ]),
  // Modulation. Rate in hertz, depth and feedback as percentages.
  chainP("Lush Chorus", "Modulation", [
    fx("modulation", { kind: 0, rate: 0.5, depth: 55, voices: 4, chorus_delay: 20, width: 100, spread: 60, wet: 45 }),
  ]),
  chainP("Subtle Chorus", "Modulation", [
    fx("modulation", { kind: 0, rate: 0.3, depth: 30, voices: 2, chorus_delay: 15, width: 80, wet: 30 }),
  ]),
  chainP("Jet Flanger", "Modulation", [
    fx("modulation", { kind: 1, rate: 0.2, depth: 90, feedback: 70, flange_delay: 1, width: 100, wet: 50 }),
  ]),
  chainP("Soft Flanger", "Modulation", [
    fx("modulation", { kind: 1, rate: 0.4, depth: 50, feedback: 25, flange_delay: 2.5, width: 90, wet: 40 }),
  ]),
  chainP("Slow Phaser", "Modulation", [
    fx("modulation", { kind: 2, rate: 0.15, depth: 70, feedback: 30, stages: 2, center: 600, width: 100, wet: 50 }),
  ]),
  chainP("Fast Phaser", "Modulation", [
    fx("modulation", { kind: 2, rate: 2.5, depth: 60, feedback: 40, stages: 1, center: 900, width: 100, wet: 50 }),
  ]),
  chainP("Vocal Double", "Modulation", [
    fx("modulation", { kind: 3, adt_offset: 14, lfo_range: 1.2, lfo_rate: 0.4, lfo_shape: 2, src_pan: -35, adt_pan: 35, adt_level: -2, wet: 50 }),
  ]),
  chainP("Wide Guitar Double", "Modulation", [
    fx("modulation", { kind: 3, adt_offset: 18, lfo_range: 2, lfo_rate: 0.3, lfo_shape: 1, src_pan: -80, adt_pan: 80, adt_level: 0, wet: 50 }),
  ]),
  // Tone and stereo.
  chainP("Warm Tube Drive", "Tone and stereo", [
    fx("tube_preamp", { input: 4, drive: 45, low: 1, high: -1, transformer: 1, output: -3 }),
  ]),
  chainP("Clean Tube Warmth", "Tone and stereo", [
    fx("tube_preamp", { input: 0, drive: 20, low: 0, high: 0, transformer: 1, output: 0 }),
  ]),
  chainP("Crunchy Tube", "Tone and stereo", [
    fx("tube_preamp", { input: 10, drive: 75, low: 2, high: 1, transformer: 1, output: -8 }),
  ]),
  chainP("Vox - Clean Podcast", "Tone and stereo", [
    fx("vox_strip", { low_cut: 1, dyn_style: 1, comp: 35, deess: 30, tone_style: 0, low: 0, mid: 1, high: 1, air: 15 }),
  ]),
  chainP("Vox - Warm Radio", "Tone and stereo", [
    fx("vox_strip", { low_cut: 1, dyn_style: 2, comp: 50, deess: 35, tone_style: 1, low: 2, mid: -1, high: 1, air: 10 }),
  ]),
  chainP("Vox - Modern Pop", "Tone and stereo", [
    fx("vox_strip", { low_cut: 1, dyn_style: 0, comp: 60, deess: 40, tone_style: 2, low: -1, mid: 0, high: 2, air: 40, fx_on: 1, fx_style: 1, fx: 25 }),
  ]),
  chainP("Vox - Lo-Fi Radio", "Tone and stereo", [
    fx("vox_strip", { low_cut: 1, dyn_style: 2, comp: 70, deess: 20, tone_style: 1, fx_on: 1, fx_style: 3, fx: 80 }),
  ]),
  chainP("Wider Stereo", "Tone and stereo", [
    fx("stereo_width", { pan: 0, width: 150 }),
  ]),
  chainP("Narrow for Mono Safety", "Tone and stereo", [
    fx("stereo_width", { pan: 0, width: 60 }),
  ]),
  chainP("Mono", "Tone and stereo", [
    fx("stereo_width", { pan: 0, width: 0 }),
  ]),
  chainP("Telephone - Receiver", "EQ", [
    fx("parametric_eq", {
      hp_on: 1,
      hp_freq: 300,
      hp_slope: 2,
      lp_on: 1,
      lp_freq: 3400,
      lp_slope: 2,
      b2_freq: 1500,
      b2_gain: 3,
      b2_q: 0.8,
    }),
  ]),
  chainP(
    "Telephone - Voice Mail",
    "EQ",
    [
      fx("parametric_eq", {
        hp_on: 1,
        hp_freq: 400,
        hp_slope: 2,
        lp_on: 1,
        lp_freq: 3000,
        lp_slope: 2,
        b2_freq: 1800,
        b2_gain: 5,
        b2_q: 0.7,
      }),
      comp(-24, 4, 6),
    ],
    {},
  ),
];

/** The chain a preset gives, with fresh slot ids. Unknown effects are dropped. */
export function presetChain(
  p: ChainPreset,
  known: ReadonlySet<string>,
  newId: () => string,
): EffectChainDoc {
  return {
    ...p.chain,
    slots: p.chain.slots
      .filter((s) => known.has(s.effect))
      .map((s) => ({
        id: newId(),
        effect: s.effect,
        enabled: s.enabled !== false,
        params: { ...s.params },
        preset: p.name,
        ...(s.plugin_state ? { plugin_state: s.plugin_state } : {}),
      })),
  };
}

/** What choosing a preset does to the current chain. Null when it would overflow. */
export function applyPreset(
  current: EffectChainDoc,
  p: ChainPreset,
  known: ReadonlySet<string>,
  newId: () => string,
): EffectChainDoc | null {
  const next = presetChain(p, known, newId);
  if (!addsOne(p)) return next;
  const slot = next.slots[0];
  return slot ? addSlot(current, slot) : current;
}

/** A user preset saved from the current chain (slot ids are not kept). */
export function presetFrom(name: string, chain: EffectChainDoc): ChainPreset {
  return {
    name,
    group: "Voice chains",
    chain: {
      ...chain,
      slots: chain.slots.map((s) => ({
        effect: s.effect,
        params: { ...s.params },
        ...(s.plugin_state ? { plugin_state: s.plugin_state } : {}),
        ...(s.enabled === false ? { enabled: false } : {}),
      })),
    },
  };
}

/** Saved user presets, kept only when well formed. */
export function sanitizePresets(raw: unknown): ChainPreset[] {
  if (!Array.isArray(raw)) return [];
  const out: ChainPreset[] = [];
  for (const r of raw) {
    if (!r || typeof r !== "object") continue;
    const v = r as Record<string, unknown>;
    const c = v.chain as Record<string, unknown> | undefined;
    if (
      typeof v.name !== "string" ||
      !v.name.trim() ||
      !c ||
      !Array.isArray(c.slots)
    )
      continue;
    const num = (x: unknown, d: number) =>
      typeof x === "number" && Number.isFinite(x) ? x : d;
    out.push({
      name: v.name,
      group: "Voice chains",
      chain: {
        enabled: c.enabled !== false,
        input_gain_db: num(c.input_gain_db, 0),
        output_gain_db: num(c.output_gain_db, 0),
        mix: num(c.mix, 1),
        slots: (c.slots as unknown[])
          .filter(
            (s): s is { effect: string; params: Record<string, number> } =>
              !!s && typeof (s as { effect?: unknown }).effect === "string",
          )
          .slice(0, MAX_SLOTS)
          .map((s) => {
            const x = s as { plugin_state?: unknown; enabled?: unknown };
            return {
              effect: s.effect,
              params:
                typeof s.params === "object" && s.params ? { ...s.params } : {},
              ...(typeof x.plugin_state === "string" && x.plugin_state ? { plugin_state: x.plugin_state } : {}),
              ...(x.enabled === false ? { enabled: false } : {}),
            };
          }),
      },
    });
  }
  return out;
}

/** A plug-in as the Add effect menu needs it. */
export interface MenuPlugin {
  effect: string;
  name: string;
  vendor: string;
  /** VST3 categories, for example "Fx|Reverb". */
  kind: string;
}

/**
 * Plug-ins grouped for the Add effect menu: by type (the first category
 * after "Fx", "Other" when there is none) and by maker, each sorted by name.
 */
export function pluginMenuGroups(plugins: readonly MenuPlugin[]): { types: [string, MenuPlugin[]][]; makers: [string, MenuPlugin[]][] } {
  const group = (key: (p: MenuPlugin) => string) => {
    const m = new Map<string, MenuPlugin[]>();
    for (const p of plugins) {
      const k = key(p);
      m.set(k, [...(m.get(k) ?? []), p]);
    }
    return [...m.entries()]
      .sort(([a], [b]) => (a === "Other" ? 1 : b === "Other" ? -1 : a.localeCompare(b, undefined, { sensitivity: "base" })))
      .map(([k, list]): [string, MenuPlugin[]] => [k, [...list].sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" }))]);
  };
  const type = (p: MenuPlugin) => {
    const parts = p.kind
      .split("|")
      .map((s) => s.trim())
      // "Fx" and "Instrument" say what every plug-in in that list is anyway.
      .filter((s) => s && s.toLowerCase() !== "fx" && s.toLowerCase() !== "instrument");
    return parts[0] ?? "Other";
  };
  return { types: group(type), makers: group((p) => p.vendor.trim() || "Other") };
}
