// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Keeps the session in step with running VST3 plug-ins. What the user
 * sets in a plug-in's own window lives in its host process; this brings
 * it back into the slot (controls as `p<id>` values, everything else as
 * `plugin_state`) as undoable "Plug-in Settings" edits (FX Chain
 * slots and MIDI tracks' instruments alike), so it is saved
 * with the session and heard in exports. It also passes on plug-ins that
 * stopped, and restarts playback when a plug-in's look-ahead changes.
 */

import type { PluginEvents } from "./core";
import type { EffectChainDoc, EffectSlotDoc, EffectTarget, Op, SessionDoc } from "../stores/session";

/** Every MIDI track's instrument, with its track. */
export function instrumentSlots(doc: SessionDoc): { trackId: string; slot: EffectSlotDoc }[] {
  return doc.tracks.flatMap((t) => (t.instrument ? [{ trackId: t.id, slot: t.instrument }] : []));
}

/** Every plug-in slot: chain slots and instruments. */
export function pluginSlots(doc: SessionDoc): EffectSlotDoc[] {
  return [...allSlots(doc).map((x) => x.slot), ...instrumentSlots(doc).map((x) => x.slot)].filter(isPluginSlot);
}

/** Every chain slot in the session, with whose chain it is. */
export function allSlots(doc: SessionDoc): { target: EffectTarget; chain: EffectChainDoc; slot: EffectSlotDoc }[] {
  const out: { target: EffectTarget; chain: EffectChainDoc; slot: EffectSlotDoc }[] = [];
  const add = (target: EffectTarget, chain: EffectChainDoc | undefined) => {
    for (const slot of chain?.slots ?? []) out.push({ target, chain: chain!, slot });
  };
  for (const t of doc.tracks) {
    add({ kind: "track", id: t.id }, t.effects);
    for (const c of t.clips) add({ kind: "clip", id: c.id }, c.effects);
  }
  for (const b of doc.buses ?? []) add({ kind: "bus", id: b.id }, b.effects);
  add({ kind: "master" }, doc.master?.effects);
  return out;
}

export const isPluginSlot = (s: EffectSlotDoc) => s.effect.startsWith("vst3:");

/**
 * The edits that bring plug-in events and fetched settings into the
 * session: one `set_effects` per chain touched. `states` holds settings
 * fetched from running plug-ins, by slot id.
 */
export function syncOps(doc: SessionDoc, events: PluginEvents[], states: ReadonlyMap<string, string>): Op[] {
  const byTarget = new Map<string, { target: EffectTarget; chain: EffectChainDoc }>();
  const keyOf = (t: EffectTarget) => ("id" in t ? `${t.kind}:${t.id}` : t.kind);
  const slots = allSlots(doc);
  const instruments = instrumentSlots(doc);
  const changedInstruments = new Map<string, EffectSlotDoc>();
  const touch = (slotId: string, change: (s: EffectSlotDoc) => EffectSlotDoc) => {
    const inst = instruments.find((x) => x.slot.id === slotId);
    if (inst) {
      changedInstruments.set(inst.trackId, change(changedInstruments.get(inst.trackId) ?? inst.slot));
      return;
    }
    const found = slots.find((x) => x.slot.id === slotId);
    if (!found) return;
    const k = keyOf(found.target);
    const cur = byTarget.get(k) ?? { target: found.target, chain: found.chain };
    cur.chain = { ...cur.chain, slots: cur.chain.slots.map((s) => (s.id === slotId ? change(s) : s)) };
    byTarget.set(k, cur);
  };
  for (const e of events) {
    if (e.edits.length > 0) {
      touch(e.slotId, (s) => {
        const params = { ...s.params };
        for (const [key, v] of e.edits) params[key] = v;
        return { ...s, params, preset: null };
      });
    }
  }
  for (const [slotId, data] of states) {
    touch(slotId, (s) => (s.plugin_state === data ? s : { ...s, plugin_state: data }));
  }
  const ops: Op[] = [];
  for (const { target, chain } of byTarget.values()) {
    const before = slots.find((x) => keyOf(x.target) === keyOf(target))?.chain;
    if (before && JSON.stringify(before) === JSON.stringify(chain)) continue;
    ops.push({ op: "set_effects", target, chain });
  }
  for (const [trackId, slot] of changedInstruments) {
    const before = instruments.find((x) => x.trackId === trackId)?.slot;
    if (before && JSON.stringify(before) === JSON.stringify(slot)) continue;
    ops.push({ op: "set_track", id: trackId, changes: { instrument: slot } });
  }
  return ops;
}

export interface PluginSyncDeps {
  doc: () => SessionDoc | null;
  events: () => Promise<PluginEvents[]>;
  state: (slotId: string) => Promise<string | null>;
  /** Applies the edits as one undoable step. */
  commit: (ops: Op[]) => Promise<void>;
  /** A plug-in stopped (crashed or hung); it is bypassed. */
  onStopped: (slot: EffectSlotDoc, why: string) => void;
  /** A plug-in's look-ahead changed: playback must re-align. */
  onLatency: () => void;
}

export class PluginSync {
  private timer: ReturnType<typeof setInterval> | null = null;
  private busy = false;

  constructor(private readonly deps: PluginSyncDeps) {}

  start(everyMs = 400): void {
    if (this.timer) return;
    this.timer = setInterval(() => void this.tick(), everyMs);
  }

  stop(): void {
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
  }

  /** One round: events in, settings fetched where they changed, edits out. */
  async tick(): Promise<void> {
    const doc = this.deps.doc();
    if (this.busy || !doc || pluginSlots(doc).length === 0) return;
    this.busy = true;
    try {
      const events = await this.deps.events();
      if (events.length === 0) return;
      const states = new Map<string, string>();
      for (const e of events) {
        if (e.dirty || e.windowClosed || e.edits.length > 0) {
          const s = await this.deps.state(e.slotId);
          if (s) states.set(e.slotId, s);
        }
        if (e.stopped) {
          const slot = pluginSlots(doc).find((s) => s.id === e.slotId);
          if (slot) this.deps.onStopped(slot, e.stopped);
        }
      }
      if (events.some((e) => e.latencyChanged)) this.deps.onLatency();
      const now = this.deps.doc();
      if (!now) return;
      const ops = syncOps(now, events, states);
      if (ops.length) await this.deps.commit(ops);
    } finally {
      this.busy = false;
    }
  }

  /** Fetches every running plug-in's settings into the session (before a save or an export). */
  async captureAll(): Promise<void> {
    const doc = this.deps.doc();
    if (!doc) return;
    const states = new Map<string, string>();
    for (const slot of pluginSlots(doc)) {
      const s = await this.deps.state(slot.id).catch(() => null);
      if (s && s !== slot.plugin_state) states.set(slot.id, s);
    }
    if (states.size === 0) return;
    const ops = syncOps(doc, [], states);
    if (ops.length) await this.deps.commit(ops);
  }
}
