// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

/**
 * The Parametric EQ's graph editor: which band each node on the graph
 * stands for, and the width (Q) maths for its handles.
 */

import type { EffectInfo, EffectParamInfo } from "../../app/core";
import type { EffectSlotDoc } from "../../stores/session";
import { paramValue } from "./chain";

/** One node: a filter or band, with the controls it moves. */
export interface EqNode {
  /** Short label drawn on the node: HP, L, 1, 2, 3, H, LP. */
  label: string;
  /** The band's name, as its controls are grouped. */
  name: string;
  freq: EffectParamInfo;
  gain: EffectParamInfo | null;
  q: EffectParamInfo | null;
  /** The on switch, for the filters that have one. */
  on: EffectParamInfo | null;
}

const LABELS: Record<string, string> = {
  "High-pass": "HP",
  "Low shelf": "L",
  "Band 1": "1",
  "Band 2": "2",
  "Band 3": "3",
  "High shelf": "H",
  "Low-pass": "LP",
};

/** The nodes of an EQ, in order from low to high, from its controls' groups. */
export function eqNodes(info: EffectInfo): EqNode[] {
  const groups = [...new Set(info.params.map((p) => p.group).filter((g): g is string => !!g))];
  return groups.flatMap((g) => {
    const ps = info.params.filter((p) => p.group === g);
    const freq = ps.find((p) => p.unit === "Hz");
    if (!freq) return [];
    return [
      {
        label: LABELS[g] ?? g.slice(0, 2),
        name: g,
        freq,
        gain: ps.find((p) => p.unit === "dB") ?? null,
        q: ps.find((p) => p.key.endsWith("_q")) ?? null,
        on: ps.find((p) => p.toggle) ?? null,
      },
    ];
  });
}

/** Whether a node is in use: always, or when its on switch is on. */
export function nodeOn(slot: EffectSlotDoc, n: EqNode): boolean {
  return !n.on || paramValue(slot, n.on) >= 0.5;
}

/** A peaking band's width in octaves for a Q. */
export function octavesOf(q: number): number {
  return (2 / Math.LN2) * Math.asinh(1 / (2 * Math.max(1e-3, q)));
}

/** The Q for a width in octaves. */
export function qOf(octaves: number): number {
  const r = Math.pow(2, Math.max(0.01, octaves));
  return Math.sqrt(r) / (r - 1);
}

/** Where a band's width handles sit: the frequencies half its width either side. */
export function widthEdges(freq: number, q: number): [number, number] {
  const half = Math.pow(2, octavesOf(q) / 2);
  return [freq / half, freq * half];
}

/** The Q a width handle dragged to `at` Hz gives a band centered on `freq`. */
export function qFromEdge(freq: number, at: number): number {
  const octaves = 2 * Math.abs(Math.log2(Math.max(1, at) / Math.max(1, freq)));
  return qOf(octaves);
}

/** "1.2 kHz" or "80 Hz". */
export function formatHz(hz: number): string {
  return hz >= 1000 ? `${+(hz / 1000).toFixed(hz >= 10_000 ? 1 : 2)} kHz` : `${Math.round(hz)} Hz`;
}
