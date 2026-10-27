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

/** Pushes effects down where one now sits on or above the one before. */
function bumped(slots: EffectSlotDoc[]): EffectSlotDoc[] {
  let next = 0;
  return slots.map((s) => {
    const at = Math.max(next, s.position ?? next);
    next = at + 1;
    return at === s.position ? s : { ...s, position: at };
  });
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
  return { ...chain, slots };
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
  return { ...chain, slots };
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
    slots: { effect: string; params: Record<string, number> }[];
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
        enabled: true,
        params: { ...s.params },
        preset: p.name,
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
          .map((s) => ({
            effect: s.effect,
            params:
              typeof s.params === "object" && s.params ? { ...s.params } : {},
          })),
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
