// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Routing as the Mixer and FX Chain offer it: outputs, aux sends and
 * sidechain keys. The core refuses any routing that would feed sound
 * back into itself; these helpers leave those choices out of the lists
 * so the user never meets the refusal.
 */

import type { BusDoc, EffectChainDoc, Op, OutputDoc, SendDoc, SessionDoc } from "../../stores/session";
import { settings } from "../../stores/settings";

/** Id the Mixer and FX Chain use for the master. */
export const MASTER_ID = "master";

interface Owner {
  id: string;
  output?: OutputDoc | undefined;
  sends?: SendDoc[] | undefined;
  effects?: EffectChainDoc | undefined;
}

function owners(doc: SessionDoc): Owner[] {
  return [...doc.tracks, ...(doc.buses ?? [])];
}

/** Every edge as [from, to]: outputs and sends to buses, and keys (source to listener). */
function edges(doc: SessionDoc): [string, string][] {
  const buses = new Set((doc.buses ?? []).map((b) => b.id));
  const all = new Set(owners(doc).map((o) => o.id));
  const out: [string, string][] = [];
  for (const o of owners(doc)) {
    if (o.output?.type === "bus" && buses.has(o.output.bus_id)) out.push([o.id, o.output.bus_id]);
    for (const s of o.sends ?? []) if (buses.has(s.bus_id)) out.push([o.id, s.bus_id]);
    for (const k of keySources(o.effects)) if (all.has(k)) out.push([k, o.id]);
  }
  return out;
}

/** The tracks and buses a chain's effects listen to. */
export function keySources(chain: EffectChainDoc | undefined): string[] {
  return (chain?.slots ?? []).flatMap((s) => (s.sidechain ? [s.sidechain] : []));
}

/** True when sound from `from` reaches `to`. */
export function reaches(doc: SessionDoc, from: string, to: string): boolean {
  const e = edges(doc);
  const stack = [from];
  const seen = new Set<string>();
  while (stack.length) {
    const n = stack.pop()!;
    if (n === to) return true;
    if (seen.has(n)) continue;
    seen.add(n);
    for (const [a, b] of e) if (a === n) stack.push(b);
  }
  return false;
}

/** Buses `owner` can output or send to without a loop. */
export function busChoices(doc: SessionDoc, owner: string): BusDoc[] {
  return (doc.buses ?? []).filter((b) => b.id !== owner && !reaches(doc, b.id, owner));
}

/** Tracks and buses an effect on `owner`'s chain can listen to without a loop. The master can listen to any. */
export function keyChoices(doc: SessionDoc, owner: string): { id: string; name: string; bus: boolean }[] {
  const ok = (id: string) => owner === MASTER_ID || (id !== owner && !reaches(doc, owner, id));
  return [
    ...doc.tracks.filter((t) => ok(t.id)).map((t) => ({ id: t.id, name: t.name, bus: false })),
    ...(doc.buses ?? []).filter((b) => ok(b.id)).map((b) => ({ id: b.id, name: b.name, bus: true })),
  ];
}

function chainActive(c: EffectChainDoc | undefined): boolean {
  if (!c || !c.enabled) return false;
  return c.input_gain_db !== 0 || c.output_gain_db !== 0 || c.slots.some((s) => s.enabled);
}

/**
 * True when the mix runs on the render thread (a track routed to a bus,
 * a send, a master chain or a sidechain). Track and bus faders then
 * react about half a second late; the master fader stays instant.
 */
export function isRouted(doc: SessionDoc | null): boolean {
  if (!doc) return false;
  return (
    chainActive(doc.master?.effects) ||
    doc.tracks.some((t) => t.output?.type === "bus" || (t.sends ?? []).some((s) => s.enabled) || keySources(t.effects).length > 0)
  );
}

/** Where `output` goes, as a label. */
export function outputName(doc: SessionDoc, output: OutputDoc | undefined): string {
  if (output?.type !== "bus") return "Master";
  return doc.buses?.find((b) => b.id === output.bus_id)?.name ?? "Master";
}

/** A new post-fader send to `busId` at 0 dB. */
export function newSend(busId: string, id: string): SendDoc {
  return { id, bus_id: busId, level_db: 0, pan: 0, pre_fader: false, enabled: true };
}

export function setSend(sends: SendDoc[], id: string, patch: Partial<SendDoc>): SendDoc[] {
  return sends.map((s) => (s.id === id ? { ...s, ...patch } : s));
}

export function removeSend(sends: SendDoc[], id: string): SendDoc[] {
  return sends.filter((s) => s.id !== id);
}

/** Bus changes as one op. With exclusive solo on, soloing a bus unsolos every other track and bus. */
export function busOp(doc: SessionDoc | null, id: string, changes: Record<string, unknown>, exclusive = settings.current.soloExclusive): Op {
  const op = { op: "set_bus", id, changes } as Op;
  if (!exclusive || changes.solo !== true || !doc) return op;
  const ops: Op[] = [
    op,
    ...doc.tracks.filter((t) => t.solo).map((t) => ({ op: "set_track", id: t.id, changes: { solo: false } }) as Op),
    ...(doc.buses ?? []).filter((b) => b.id !== id && b.solo).map((b) => ({ op: "set_bus", id: b.id, changes: { solo: false } }) as Op),
  ];
  return ops.length === 1 ? op : { op: "batch", ops };
}
