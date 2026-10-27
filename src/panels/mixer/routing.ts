// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Routing as the Mixer and Audio FX offer it: outputs, aux sends and
 * sidechain keys. The core refuses any routing that would feed sound
 * back into itself; these helpers leave those choices out of the lists
 * so the user never meets the refusal.
 */

import type { BusDoc, EffectChainDoc, Op, OutputDoc, SendDoc, SessionDoc } from "../../stores/session";
import { settings } from "../../stores/settings";

/** Id the Mixer and Audio FX use for the master. */
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

/** Tracks and buses whose output reaches bus `id`, directly or through other buses. Sends are left out. */
export function feeders(doc: SessionDoc, id: string): { tracks: string[]; buses: string[] } {
  const tracks: string[] = [];
  const buses: string[] = [];
  const stack = [id];
  const seen = new Set<string>([id]);
  while (stack.length) {
    const to = stack.pop()!;
    for (const t of doc.tracks) if (t.output?.type === "bus" && t.output.bus_id === to) tracks.push(t.id);
    for (const b of doc.buses ?? []) {
      if (b.output?.type !== "bus" || b.output.bus_id !== to || seen.has(b.id)) continue;
      seen.add(b.id);
      buses.push(b.id);
      stack.push(b.id);
    }
  }
  return { tracks, buses };
}

/**
 * Bus changes as one op. Soloing or unsoloing a bus does the same to every
 * track and bus feeding it. With exclusive solo on, soloing a bus also
 * unsolos every other track and bus.
 */
export function busOp(doc: SessionDoc | null, id: string, changes: Record<string, unknown>, exclusive = settings.current.soloExclusive): Op {
  const op = { op: "set_bus", id, changes } as Op;
  if (typeof changes.solo !== "boolean" || !doc) return op;
  const solo = changes.solo;
  const feed = feeders(doc, id);
  const ft = new Set(feed.tracks);
  const fb = new Set(feed.buses);
  const ops: Op[] = [op];
  for (const t of doc.tracks) {
    if (ft.has(t.id)) {
      if (t.solo !== solo) ops.push({ op: "set_track", id: t.id, changes: { solo } } as Op);
    } else if (solo && exclusive && t.solo) ops.push({ op: "set_track", id: t.id, changes: { solo: false } } as Op);
  }
  for (const b of doc.buses ?? []) {
    if (b.id === id) continue;
    if (fb.has(b.id)) {
      if (b.solo !== solo) ops.push({ op: "set_bus", id: b.id, changes: { solo } } as Op);
    } else if (solo && exclusive && b.solo) ops.push({ op: "set_bus", id: b.id, changes: { solo: false } } as Op);
  }
  return ops.length === 1 ? op : { op: "batch", ops };
}
