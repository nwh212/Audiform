// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Host profiles: a per-person starting point for a voice track, a
 * Voice Chain with the breath gate set for how close and how loud that
 * person usually is. Remote Guest also marks the track as a guest track
 * (Guest track mode): softer gate, heavier noise reduction and a
 * phone-band EQ that keeps call audio clean. Applying one replaces the
 * track's chain, as one undoable step.
 */

import { BUILTIN_PRESETS, presetChain, type ChainPreset } from "../panels/voice-chain/chain";
import type { EffectChainDoc, Op, TrackDoc } from "../stores/session";

export interface HostProfile {
  id: string;
  name: string;
  /** Who it suits, for the menu tooltip. */
  note: string;
  gateDb: number;
  role: "standard" | "guest";
}

export const HOST_PROFILES: readonly HostProfile[] = [
  { id: "close-mic", name: "Close Mic", note: "A strong voice right on the mic.", gateDb: -14, role: "standard" },
  { id: "standard-mic", name: "Standard Mic", note: "A typical speaker at normal mic distance.", gateDb: -20, role: "standard" },
  { id: "remote-guest", name: "Remote Guest", note: "Someone calling in over the internet or phone.", gateDb: -30, role: "guest" },
];

export function profileById(id: string | null | undefined): HostProfile | undefined {
  return HOST_PROFILES.find((p) => p.id === id);
}

const preset = (name: string): ChainPreset => {
  const p = BUILTIN_PRESETS.find((x) => x.name === name);
  if (!p) throw new Error(`Missing preset ${name}`);
  return p;
};

/** The chain a profile puts on the track. */
export function profileChain(p: HostProfile, newId: () => string): EffectChainDoc {
  const base = presetChain(preset(p.role === "guest" ? "Remote Guest Cleanup" : "Podcast Voice"), ALL, newId);
  // The gate goes last, set for this profile (the guest chain already has one).
  const slots = base.slots.filter((s) => s.effect !== "altaudio.breath_gate");
  if (p.role === "guest") {
    // Phone band: the low-pass keeps call-codec fizz out above 7 kHz.
    const eq = slots.find((s) => s.effect === "altaudio.parametric_eq");
    if (eq) eq.params = { ...eq.params, lp_on: 1, lp_freq: 7000 };
  }
  slots.push({
    id: newId(),
    effect: "altaudio.breath_gate",
    enabled: true,
    params: { threshold: p.gateDb, attack: 2, hold: 50, release: 200, range: -30 },
    preset: `${p.name} profile`,
  });
  return { ...base, slots: slots.map((s) => ({ ...s, preset: `${p.name} profile` })) };
}

// Profiles only use built-in effects, so every effect counts as known.
const ALL: ReadonlySet<string> = { has: () => true } as unknown as ReadonlySet<string>;

/** The edit that applies `p` to `track`, or clears the profile when `p` is null. */
export function profileOp(track: TrackDoc, p: HostProfile | null, newId: () => string): Op {
  if (!p) return { op: "set_track", id: track.id, changes: { host_profile_id: "", role: "standard" } };
  return {
    op: "batch",
    ops: [
      { op: "set_track", id: track.id, changes: { host_profile_id: p.id, role: p.role } },
      { op: "set_effects", target: { kind: "track", id: track.id }, chain: profileChain(p, newId) },
    ],
  };
}
