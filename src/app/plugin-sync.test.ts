// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it, vi } from "vitest";
import type { SessionDoc } from "../stores/session";
import { PluginSync, syncOps } from "./plugin-sync";
import type { PluginEvents } from "./core";

const chain = (slots: object[]) => ({ enabled: true, input_gain_db: 0, output_gain_db: 0, mix: 1, slots });
const plug = (id: string, extra: object = {}) => ({ id, effect: "vst3:ABC", enabled: true, params: { p0: 1 }, preset: null, ...extra });
const doc = {
  tracks: [{ id: "t", clips: [{ id: "c", effects: chain([]) }], effects: chain([plug("s1"), { id: "g", effect: "altaudio.gain", enabled: true, params: {}, preset: null }]) }],
  buses: [{ id: "b", effects: chain([plug("s2")]) }],
  master: { effects: chain([]) },
} as unknown as SessionDoc;
const ev = (slotId: string, e: Partial<PluginEvents> = {}): PluginEvents => ({ slotId, edits: [], dirty: false, windowClosed: false, latencyChanged: false, stopped: null, ...e });

describe("plug-in sync", () => {
  it("brings window edits and settings into the right chains", () => {
    const ops = syncOps(doc, [ev("s1", { edits: [["p0", 0.25], ["p3", 1]] })], new Map([["s2", "STATE"]]));
    expect(ops).toHaveLength(2);
    const [a, b] = ops as { op: "set_effects"; target: unknown; chain: { slots: { id: string; params: object; plugin_state?: string }[] } }[];
    expect(a!.target).toEqual({ kind: "track", id: "t" });
    expect(a!.chain.slots[0]!.params).toEqual({ p0: 0.25, p3: 1 });
    expect(a!.chain.slots[1]!.id).toBe("g");
    expect(b!.target).toEqual({ kind: "bus", id: "b" });
    expect(b!.chain.slots[0]!.plugin_state).toBe("STATE");
  });

  it("keeps a MIDI track's instrument settings with the track", () => {
    const d = { ...doc, tracks: [...doc.tracks, { id: "k", kind: "midi", clips: [], effects: chain([]), instrument: plug("i1") }] } as unknown as SessionDoc;
    const ops = syncOps(d, [ev("i1", { edits: [["p0", 0.5]] })], new Map([["i1", "SYNTH"]]));
    expect(ops).toEqual([{ op: "set_track", id: "k", changes: { instrument: { ...plug("i1"), params: { p0: 0.5 }, plugin_state: "SYNTH" } } }]);
  });

  it("bypasses or turns on an effect from its window's power button", () => {
    const ops = syncOps(doc, [ev("s2", { bypass: true })], new Map()) as { op: string; chain: { slots: { enabled: boolean }[] } }[];
    expect(ops).toHaveLength(1);
    expect(ops[0]!.chain.slots[0]!.enabled).toBe(false);
    expect(syncOps(doc, [ev("s2", { bypass: false })], new Map())).toEqual([]);
  });

  it("makes no edit when nothing changed", () => {
    expect(syncOps(doc, [ev("s1", { edits: [["p0", 1]] })], new Map())).toEqual([]);
    expect(syncOps(doc, [ev("gone", { edits: [["p0", 0]] })], new Map())).toEqual([]);
  });

  it("fetches settings after window changes, reports stops and latency", async () => {
    const commit = vi.fn(async () => {});
    const onStopped = vi.fn();
    const onLatency = vi.fn();
    const sync = new PluginSync({
      doc: () => doc,
      events: async () => [ev("s1", { windowClosed: true }), ev("s2", { stopped: "It crashed.", latencyChanged: true })],
      state: async (id) => (id === "s1" ? "NEW" : null),
      commit,
      onStopped,
      onLatency,
    });
    await sync.tick();
    expect(onStopped).toHaveBeenCalledWith(expect.objectContaining({ id: "s2" }), "It crashed.");
    expect(onLatency).toHaveBeenCalled();
    expect(commit).toHaveBeenCalledTimes(1);
    const ops = (commit.mock.calls[0] as unknown as [{ chain: { slots: { plugin_state?: string }[] } }[]])[0];
    expect(ops[0]!.chain.slots[0]!.plugin_state).toBe("NEW");
  });
});
