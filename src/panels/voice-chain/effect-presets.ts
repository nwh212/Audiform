// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Presets for one effect: its settings saved under a name, kept in the
 * preferences by effect id. (VST3 plug-ins keep theirs as preset files
 * in the standard folders instead; see the core's `plugins/presets.rs`.)
 */

import type { EffectSlotDoc } from "../../stores/session";

export const KEY_EFFECT_PRESETS = "effect.presets";

export interface EffectPreset {
  name: string;
  params: Record<string, number>;
  /** The effect's own data (the Amp Model's capture and cabinet), when it has some. */
  data?: string | null;
}

export type EffectPresets = Record<string, EffectPreset[]>;

/** Presets as stored, with anything malformed dropped. */
export function sanitizeEffectPresets(raw: unknown): EffectPresets {
  if (!raw || typeof raw !== "object") return {};
  const out: EffectPresets = {};
  for (const [effect, list] of Object.entries(raw as Record<string, unknown>)) {
    if (!Array.isArray(list)) continue;
    const ok = list.flatMap((p): EffectPreset[] => {
      if (!p || typeof p !== "object") return [];
      const { name, params, data } = p as Record<string, unknown>;
      if (typeof name !== "string" || !name.trim() || !params || typeof params !== "object") return [];
      const nums = Object.fromEntries(Object.entries(params as Record<string, unknown>).filter((e): e is [string, number] => typeof e[1] === "number" && Number.isFinite(e[1])));
      return [{ name: name.trim(), params: nums, ...(typeof data === "string" ? { data } : {}) }];
    });
    if (ok.length) out[effect] = ok;
  }
  return out;
}

/** Saves `slot`'s settings as preset `name` for its effect, replacing one of the same name. */
export function savePreset(all: EffectPresets, slot: EffectSlotDoc, name: string): EffectPresets {
  const p: EffectPreset = { name, params: { ...slot.params }, ...(slot.data ? { data: slot.data } : {}) };
  const list = [...(all[slot.effect] ?? []).filter((x) => x.name !== name), p].sort((a, b) => a.name.localeCompare(b.name));
  return { ...all, [slot.effect]: list };
}

export function deletePreset(all: EffectPresets, effect: string, name: string): EffectPresets {
  const list = (all[effect] ?? []).filter((x) => x.name !== name);
  const next = { ...all };
  if (list.length) next[effect] = list;
  else delete next[effect];
  return next;
}

/** The slot change that applies a preset: its values over the defaults, and its name. */
export function presetChange(p: EffectPreset, defaults: Record<string, number>): Partial<EffectSlotDoc> {
  return { params: { ...defaults, ...p.params }, data: p.data ?? null, preset: p.name };
}
