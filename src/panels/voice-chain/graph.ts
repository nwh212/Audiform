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

/**
 * Effects whose window shows the plain response graph. The Parametric EQ
 * and the Notch Filter have graph editors of their own.
 */
export const GRAPHED = new Set([
  "altaudio.band_eq_10",
  "altaudio.band_eq_20",
  "altaudio.band_eq_30",
  "altaudio.hum_cut",
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

/** A line on a frequency graph: every 1 to 9 step of each decade, labeled where there is room. */
export interface FreqTick {
  f: number;
  /** Its label ("Hz" at the left end, "1k" and so on), or null when it is a line only. */
  label: string | null;
  /** 100 Hz, 1 kHz and 10 kHz: drawn stronger. */
  major: boolean;
}

/** The frequency ruler's lines and labels for a graph `width` pixels wide (20 Hz to 20 kHz). */
export function freqTicks(width: number): FreqTick[] {
  const out: FreqTick[] = [];
  let lastX = -Infinity;
  for (const decade of [10, 100, 1000, 10_000]) {
    for (let k = 1; k <= 9; k++) {
      const f = decade * k;
      if (f < GRAPH_LOW || f > GRAPH_HIGH) continue;
      const x = xOf(f) * width;
      const major = k === 1 && decade >= 100;
      const text = f === GRAPH_LOW ? "Hz" : f >= 1000 ? `${f / 1000}k` : String(f);
      // Labels need room; the round ones always get theirs.
      const room = major || f === GRAPH_LOW || f === GRAPH_HIGH ? 0 : 20 + text.length * 2;
      const label = x - lastX >= room && x <= width - 14 + (f === GRAPH_HIGH ? 14 : 0) ? text : null;
      if (label) lastX = x;
      out.push({ f, label, major });
    }
  }
  return out;
}
