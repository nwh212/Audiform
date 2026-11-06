// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { AutoWriter, capturedValues, holdOnce, thinPoints, TOUCH_RELEASE_MS, writeSpan } from "./auto-write";
import { WRITE_LABEL, type AutomationMode, type EnvelopeDoc, type Op, type SessionDoc, type TrackDoc } from "./session";

const track = (over: Partial<TrackDoc> = {}): TrackDoc => ({
  id: "t",
  name: "Host",
  role: "standard",
  channels: "stereo",
  color: null,
  height: 96,
  volume_db: -6,
  pan: 0,
  muted: false,
  solo: false,
  record_armed: false,
  clips: [],
  effects: { enabled: true, input_gain_db: 0, output_gain_db: 0, mix: 1, slots: [{ id: "s1", effect: "altaudio.gain", enabled: true, params: { gain: 0 }, preset: null }] },
  ...over,
});

const doc = (t: TrackDoc): SessionDoc => ({
  format: "altaudio.session",
  schema_version: 1,
  id: "s",
  name: "s",
  sample_rate: 48000,
  bit_depth: 32,
  media: [],
  tracks: [t],
  markers: [],
  view: { samples_per_pixel: 1024, scroll: 0, scroll_y: 0, playhead: 0, selection: null },
  libraries: [],
});

const pt = (position: number, value: number) => ({ position, value, curve: "linear" as const });

describe("capturedValues", () => {
  it("reads fader, pan, spot and effect moves, and nothing else", () => {
    const d = doc(track({ space: { x: 0, y: 1, z: 0, size: 0, width: 1 } }));
    expect(capturedValues(d, { op: "set_track", id: "t", changes: { volume_db: -3 } })).toEqual([{ owner: "t", target: { type: "volume" }, value: -3 }]);
    expect(capturedValues(d, { op: "set_track", id: "t", changes: { pan: 40 } })).toEqual([{ owner: "t", target: { type: "pan" }, value: 40 }]);
    expect(capturedValues(d, { op: "set_track", id: "t", changes: { space: { x: 0.5, y: 1, z: 0, size: 0, width: 1 } } })).toEqual([{ owner: "t", target: { type: "space", param: "x" }, value: 0.5 }]);
    const chain = { ...d.tracks[0]!.effects!, slots: [{ ...d.tracks[0]!.effects!.slots[0]!, params: { gain: 6 } }] };
    expect(capturedValues(d, { op: "set_effects", target: { kind: "track", id: "t" }, chain })).toEqual([{ owner: "t", target: { type: "effect_param", slot_id: "s1", param: "gain" }, value: 6 }]);
    expect(capturedValues(d, { op: "set_track", id: "t", changes: { name: "Guest" } })).toEqual([]);
    // A new effect is not a move.
    expect(capturedValues(d, { op: "set_effects", target: { kind: "track", id: "t" }, chain: { ...chain, slots: [] } })).toEqual([]);
  });
});

describe("writeSpan", () => {
  it("replaces the stretch written over and keeps the rest", () => {
    const points = [pt(0, 0), pt(100, 1), pt(200, 2), pt(300, 3)];
    expect(writeSpan(points, 100, 250, 9)).toEqual([pt(0, 0), pt(100, 1), pt(250, 9), pt(300, 3)]);
    // With a start value, the stretch begins from where the lane was.
    expect(writeSpan([pt(300, 3)], 100, 250, 9, 5)).toEqual([pt(100, 5), pt(250, 9), pt(300, 3)]);
  });
});

describe("thinPoints", () => {
  it("drops points on a straight line inside the stretch and keeps the corners", () => {
    const points = [pt(0, 0), pt(100, 1), pt(200, 2), pt(300, 3), pt(400, 0), pt(500, 0), pt(900, 7)];
    expect(thinPoints(points, 0, 500, 0.01)).toEqual([pt(0, 0), pt(300, 3), pt(400, 0), pt(500, 0), pt(900, 7)]);
  });
});

/** A session store stand-in that applies set_envelopes and set_track volume to its one track. */
function harness(mode: AutomationMode, envelopes: EnvelopeDoc[] = []) {
  let now = 0;
  let d = doc(track({ automation_mode: mode, envelopes }));
  const edits: { label: string; op: Op }[] = [];
  const apply = (op: Op) => {
    if (op.op === "batch") op.ops.forEach(apply);
    else if (op.op === "set_envelopes") d = { ...d, tracks: d.tracks.map((t) => (t.id === op.owner ? { ...t, envelopes: op.envelopes } : t)) };
    else if (op.op === "set_track" && op.changes.volume_db !== undefined) d = { ...d, tracks: d.tracks.map((t) => (t.id === op.id ? { ...t, volume_db: op.changes.volume_db! } : t)) };
  };
  const session = {
    get current() {
      return d;
    },
    edit: async (label: string, op: Op) => {
      edits.push({ label, op });
      apply(op);
    },
  };
  const w = new AutoWriter({
    session,
    runner: () => ({ execute: async () => {} }),
    rangeOf: () => ({ top: 12, bottom: -60 }),
    restOf: (o) => o.volume_db,
    now: () => now,
  });
  /** A fader move at playhead `pos`, as the header would make it (through rewrite, then applied). */
  const move = (pos: number, db: number) => {
    w.onTransport({ playing: true, position: pos });
    const r = w.rewrite({ op: "set_track", id: "t", changes: { volume_db: db } }, "Track Volume");
    if (r) {
      edits.push(r);
      apply(r.op);
    }
    return r;
  };
  const lane = () => d.tracks[0]!.envelopes?.find((e) => e.target.type === "volume");
  return { w, move, lane, edits, tick: (ms: number) => (now += ms) };
}

describe("AutoWriter", () => {
  it("leaves a track in Read alone", () => {
    const { move, lane } = harness("read");
    expect(move(1000, -3)).toBeNull();
    expect(lane()).toBeUndefined();
  });

  it("does nothing while stopped", () => {
    const { w } = harness("touch");
    w.onTransport({ playing: false, position: 0 });
    expect(w.rewrite({ op: "set_track", id: "t", changes: { volume_db: -3 } }, "Track Volume")).toBeNull();
  });

  it("Touch: writes while the fader moves, then lets go and thins the pass into one undo step", () => {
    const { w, move, lane, edits, tick } = harness("touch", [{ id: "v", target: { type: "volume" }, enabled: true, points: [pt(0, 0), pt(100_000, 0)] }]);
    const r = move(10_000, -3);
    expect(r?.label).toBe(WRITE_LABEL);
    expect(r?.op.op).toBe("batch");
    // The first move starts from where the lane was (0 dB) and ramps to the new value.
    expect(lane()!.points).toEqual([pt(0, 0), pt(10_000, -3), pt(100_000, 0)]);
    tick(100);
    move(12_000, -6);
    tick(100);
    move(14_000, -9);
    expect(w.owners).toEqual(["t"]);
    // Let go: no moves for a while, and play goes on.
    tick(TOUCH_RELEASE_MS + 1);
    w.onTransport({ playing: true, position: 16_000 });
    (w as unknown as { tick(): void }).tick();
    expect(w.owners).toEqual([]);
    // The straight ramp was thinned to its ends; the point after the pass stays.
    expect(lane()!.points).toEqual([pt(0, 0), pt(10_000, -3), pt(14_000, -9), pt(100_000, 0)]);
    expect(edits.every((e) => e.label === WRITE_LABEL)).toBe(true);
  });

  it("Latch: holds the last value on until play stops", () => {
    const { w, move, lane, tick } = harness("latch", [{ id: "v", target: { type: "volume" }, enabled: true, points: [pt(0, 0), pt(100_000, 0)] }]);
    move(10_000, -3);
    tick(1000);
    w.onTransport({ playing: true, position: 50_000 });
    (w as unknown as { tick(): void }).tick();
    expect(w.owners).toEqual(["t"]);
    expect(lane()!.points).toEqual([pt(0, 0), pt(10_000, -3), pt(50_000, -3), pt(100_000, 0)]);
    w.onTransport({ playing: false, position: 60_000 });
    expect(w.owners).toEqual([]);
    expect(lane()!.points).toEqual([pt(0, 0), pt(10_000, -3), pt(50_000, -3), pt(100_000, 0)]);
  });

  it("Write: records every lane from the start of play with the control's value", () => {
    const { w, lane } = harness("write", [{ id: "v", target: { type: "volume" }, enabled: true, points: [pt(0, 0), pt(100_000, 0)] }]);
    w.onTransport({ playing: true, position: 20_000 });
    expect(w.owners).toEqual(["t"]);
    w.onTransport({ playing: true, position: 40_000 });
    (w as unknown as { tick(): void }).tick();
    // The fader sits at -6 dB: that is what gets written over the old 0 dB line.
    expect(lane()!.points).toEqual([pt(0, 0), pt(20_000, -6), pt(40_000, -6), pt(100_000, 0)]);
    w.onTransport({ playing: false, position: 40_000 });
    expect(w.owners).toEqual([]);
  });

  it("makes a lane when the track has none for the moved control", () => {
    const { move, lane } = harness("latch");
    move(5_000, -12);
    expect(lane()).toBeDefined();
    expect(lane()!.points.map((p) => p.value)).toEqual([-12]);
  });
});

describe("holdOnce", () => {
  const p = (position: number, value: number) => ({ position, value, curve: "linear" as const });
  it("keeps a held stretch as its two ends, leaving later points alone", () => {
    expect(holdOnce([p(0, 1), p(10, -6), p(20, -6), p(30, -6), p(90, 0)], 30)).toEqual([p(0, 1), p(10, -6), p(30, -6), p(90, 0)]);
    expect(holdOnce([p(0, 1), p(10, -6), p(20, -3)], 20)).toEqual([p(0, 1), p(10, -6), p(20, -3)]);
  });
});
