// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * The response graph's maths: where a frequency and a level sit on the
 * graph, and back. Frequencies run 20 Hz to 20 kHz evenly in pitch.
 */

import type { EffectInfo, EffectParamInfo } from "../../app/core";

export const GRAPH_LOW = 20;
export const GRAPH_HIGH = 20_000;

/** Effects with a fixed response curve, which the core can graph. */
export const GRAPHED = new Set([
  "altaudio.parametric_eq",
  "altaudio.band_eq_10",
  "altaudio.band_eq_20",
  "altaudio.band_eq_30",
  "altaudio.hum_cut",
  "altaudio.notch",
  "altaudio.precision_filter",
  "altaudio.curve_eq",
]);

/** 0 to 1 across the graph. */
export function xOf(freq: number): number {
  return Math.log(Math.max(GRAPH_LOW, Math.min(GRAPH_HIGH, freq)) / GRAPH_LOW) / Math.log(GRAPH_HIGH / GRAPH_LOW);
}

export function freqAt(x: number): number {
  return GRAPH_LOW * Math.pow(GRAPH_HIGH / GRAPH_LOW, Math.max(0, Math.min(1, x)));
}

/** 0 (top) to 1 (bottom) for `db` in `top..bottom`. */
export function yOf(db: number, top: number, bottom: number): number {
  return Math.max(0, Math.min(1, (top - db) / (top - bottom)));
}

export function dbAt(y: number, top: number, bottom: number): number {
  return top - Math.max(0, Math.min(1, y)) * (top - bottom);
}

/** The level range shown: ±24 dB, opened out to fit deep cuts and big boosts. */
export function range(points: [number, number][], info?: EffectInfo): [number, number] {
  const gains = info?.params.filter((p) => p.unit === "dB" && !p.hidden) ?? [];
  let top = 24;
  let bottom = -24;
  for (const p of gains) {
    top = Math.max(top, Math.min(p.max, 40));
    bottom = Math.min(bottom, Math.max(p.min, -60));
  }
  for (const [, db] of points) bottom = Math.min(bottom, Math.max(-60, db));
  return [top, Math.floor(bottom / 6) * 6];
}

/** The Curve EQ's draggable points: per group, its frequency and gain controls. */
export function handles(info: EffectInfo): { name: string; freq: EffectParamInfo; gain: EffectParamInfo }[] {
  if (info.id !== "altaudio.curve_eq") return [];
  const out: { name: string; freq: EffectParamInfo; gain: EffectParamInfo }[] = [];
  for (const freq of info.params.filter((p) => p.unit === "Hz")) {
    const gain = info.params.find((p) => p.group === freq.group && p.unit === "dB");
    if (gain && freq.group) out.push({ name: freq.group, freq, gain });
  }
  return out;
}
