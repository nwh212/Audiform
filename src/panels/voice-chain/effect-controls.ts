// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

/**
 * The effect window's controls: dials and value fields, one column per
 * band or group. This file holds the maths; `EffectControls.svelte`
 * draws them.
 */

import type { EffectParamInfo } from "../../app/core";
import { clampParam } from "./chain";

/** A group of controls drawn as one column. */
export interface ControlColumn {
  name: string;
  /** The group's on switch (a toggle named "On"), shown in its heading. */
  power: EffectParamInfo | null;
  params: EffectParamInfo[];
}

/** Splits controls into columns by group, keeping their order. Hidden ones are left out. */
export function controlColumns(params: readonly EffectParamInfo[]): ControlColumn[] {
  const out: ControlColumn[] = [];
  for (const p of params) {
    if (p.hidden) continue;
    const name = p.group ?? "";
    let col = out[out.length - 1];
    if (!col || col.name !== name) {
      col = { name, power: null, params: [] };
      out.push(col);
    }
    if (name && p.toggle && p.name === "On" && !col.power) col.power = p;
    else col.params.push(p);
  }
  return out;
}

/**
 * A typed value, read in the control's unit: "1.5k" or "1.5 kHz" is
 * 1500 Hz, "-6" or "-6 dB" is -6 dB. Clamped to the control's range.
 * Null when the text has no number in it.
 */
export function parseParam(p: EffectParamInfo, text: string): number | null {
  const m = /^\s*([-+]?(?:\d+\.?\d*|\.\d+))\s*(k|khz)?/i.exec(text.replace(",", "."));
  if (!m) return null;
  let v = Number(m[1]);
  if (!Number.isFinite(v)) return null;
  if (m[2] && p.unit === "Hz") v *= 1000;
  return clampParam(p, v);
}
