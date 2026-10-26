// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Effect channel routing in the interface: the same mapping the core
 * makes (`dsp/effects/routing.rs`), so the routing panel shows where each
 * channel goes, and routing presets kept in the preferences.
 */

import type { SlotRoutingDoc } from "../../stores/session";

export const STEREO = ["L", "R"];

/** The chain channel an effect channel named `name` goes to by default. `own` is every name on the effect's side. */
export function byName(name: string, own: string[], chain: string[]): number | null {
  const find = (n: string) => {
    const i = chain.indexOf(n);
    return i >= 0 ? i : null;
  };
  const exact = find(name);
  if (exact !== null) return exact;
  const has = (n: string) => own.includes(n);
  const tries: Record<string, string[]> = {
    Ls: has("Lss") ? ["Lrs"] : ["Lss", "Lrs"],
    Rs: has("Rss") ? ["Rrs"] : ["Rss", "Rrs"],
    Lss: ["Ls"],
    Rss: ["Rs"],
    Lrs: ["Ls"],
    Rrs: ["Rs"],
    Ltf: ["Ltm"],
    Ltr: ["Ltm"],
    Rtf: ["Rtm"],
    Rtr: ["Rtm"],
    Ltm: ["Ltf"],
    Rtm: ["Rtf"],
    M: ["C", "L"],
  };
  for (const n of tries[name] ?? []) {
    const i = find(n);
    if (i !== null) return i;
  }
  return null;
}

export interface ChannelMap {
  inputs: (number | null)[];
  outputs: (number | null)[];
}

/** Where each effect channel goes: the slot's own routing, or by speaker name. */
export function resolve(r: SlotRoutingDoc | null | undefined, ins: string[], outs: string[], chain: string[]): ChannelMap {
  const own = (set: (number | null)[], names: string[]) => names.map((_, k) => {
    const c = set[k];
    return typeof c === "number" && c >= 0 && c < chain.length ? c : null;
  });
  const named = (names: string[]) => names.map((n) => byName(n, names, chain));
  return {
    inputs: r?.inputs?.length ? own(r.inputs, ins) : named(ins),
    outputs: r?.outputs?.length ? own(r.outputs, outs) : named(outs),
  };
}

/** "In: L, R | Out: L, R": the chain channels the effect hears and writes. */
export function describe(map: ChannelMap, chain: string[]): string {
  const list = (v: (number | null)[]) => v.map((c) => (c === null ? "none" : (chain[c] ?? "?"))).join(", ") || "none";
  return `In: ${list(map.inputs)} | Out: ${list(map.outputs)}`;
}

export const KEY_ROUTING_PRESETS = "effect.routingPresets";

export interface RoutingPreset {
  name: string;
  routing: SlotRoutingDoc;
}

export function sanitizeRoutingPresets(raw: unknown): RoutingPreset[] {
  if (!Array.isArray(raw)) return [];
  const chans = (v: unknown) => (Array.isArray(v) ? v.map((c) => (typeof c === "number" && c >= 0 && c < 64 ? Math.round(c) : null)) : []);
  return raw.flatMap((p): RoutingPreset[] => {
    if (!p || typeof p !== "object") return [];
    const { name, routing } = p as Record<string, unknown>;
    if (typeof name !== "string" || !name.trim() || !routing || typeof routing !== "object") return [];
    const r = routing as Record<string, unknown>;
    return [{ name: name.trim(), routing: { wide: r.wide === true, inputs: chans(r.inputs), outputs: chans(r.outputs) } }];
  });
}
