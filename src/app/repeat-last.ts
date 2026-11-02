// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Repeat Last Command in the Waveform editor: the Audio FX last applied
 * into a file, kept with each effect's settings (a plug-in's as its window
 * had them), so Shift+R applies them again as they were and Ctrl+R opens
 * them again to adjust.
 */

import type { EffectChainDoc, EffectSlotDoc } from "../stores/session";

export interface LastApplied {
  chain: EffectChainDoc;
  /** For the menu: "Repeat Parametric EQ". */
  name: string;
}

/**
 * A copy to apply or open again. Each effect gets a new id, so a plug-in
 * starts from the saved settings rather than a window that may since have
 * changed, and nothing is shared with the copy kept for next time.
 */
export function freshCopy(chain: EffectChainDoc, uuid: () => string): EffectChainDoc {
  const copy = structuredClone(chain);
  return {
    ...copy,
    slots: copy.slots.map((s): EffectSlotDoc => {
      // Its place in the old chain means nothing in the one it joins.
      const slot: EffectSlotDoc = { ...s, id: uuid() };
      delete slot.position;
      return slot;
    }),
  };
}

/** What the menu calls the effects: one effect by name, else the chain. */
export function repeatName(chain: EffectChainDoc, names: ReadonlyMap<string, string>): string {
  const only = chain.slots.length === 1 ? chain.slots[0] : undefined;
  return only ? (names.get(only.effect) ?? "Effect") : "Audio FX";
}

/** Plug-in slots: their settings are read from the running plug-in before they are kept. */
export const isPlugin = (s: EffectSlotDoc) => s.effect.startsWith("vst3:");
