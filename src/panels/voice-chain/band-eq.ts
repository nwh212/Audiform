// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import type { EffectInfo } from "../../app/core";

/** The Band EQ window's tabs: one effect each, picked by its band count. */
export const BAND_EQ_KINDS = [
  { id: "altaudio.band_eq_10", label: "10 Bands" },
  { id: "altaudio.band_eq_20", label: "20 Bands" },
  { id: "altaudio.band_eq_30", label: "30 Bands" },
] as const;

/** A band's frequency from its name ("31 Hz", "1.6 kHz"). */
export function bandHz(name: string): number | null {
  const m = /^([\d.]+)\s*(k?)Hz$/i.exec(name.trim());
  if (!m) return null;
  return Number(m[1]) * (m[2] ? 1000 : 1);
}

/**
 * The settings for another band count that keep the curve's shape: each
 * new band takes the level the old bands have at its frequency (read
 * between them, evenly in octaves), and Output carries over.
 */
export function carryBands(from: EffectInfo, values: Record<string, number>, to: EffectInfo): Record<string, number> {
  const old = from.params
    .filter((p) => p.group === "Bands")
    .map((p) => ({ hz: bandHz(p.name), v: values[p.key] ?? p.default }))
    .filter((b): b is { hz: number; v: number } => b.hz !== null)
    .sort((a, b) => a.hz - b.hz);
  const at = (hz: number) => {
    if (old.length === 0) return 0;
    if (hz <= old[0]!.hz) return old[0]!.v;
    for (let i = 1; i < old.length; i++) {
      const a = old[i - 1]!;
      const b = old[i]!;
      if (hz <= b.hz) {
        const t = Math.log(hz / a.hz) / Math.log(b.hz / a.hz);
        return a.v + (b.v - a.v) * t;
      }
    }
    return old[old.length - 1]!.v;
  };
  const out: Record<string, number> = {};
  for (const p of to.params) {
    if (p.group === "Bands") {
      const hz = bandHz(p.name);
      if (hz !== null) {
        const v = Math.round(at(hz) / p.step) * p.step;
        if (Math.abs(v) > 1e-9) out[p.key] = Math.max(p.min, Math.min(p.max, v));
      }
    } else if (values[p.key] !== undefined) out[p.key] = values[p.key]!;
  }
  return out;
}
