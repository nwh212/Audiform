// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { BusDoc, EffectChainDoc, SessionDoc, TrackDoc } from "../../stores/session";
import { busChoices, busOp, feeders, isRouted, keyChoices, keyOp, keyTargets, MASTER_ID, newSend, outputName, reaches, removeSend, setSend } from "./routing";

const chain = (sidechain?: string): EffectChainDoc => ({
  enabled: true,
  input_gain_db: 0,
  output_gain_db: 0,
  mix: 1,
  slots: sidechain ? [{ id: "s", effect: "altaudio.dynamics", enabled: true, params: {}, preset: null, sidechain }] : [],
});
const track = (id: string, extra: Partial<TrackDoc> = {}) => ({ id, name: id, solo: false, clips: [], ...extra }) as unknown as TrackDoc;
const bus = (id: string, extra: Partial<BusDoc> = {}) =>
  ({ id, name: id, solo: false, output: { type: "master" }, sends: [], effects: chain(), ...extra }) as unknown as BusDoc;
const session = (tracks: TrackDoc[], buses: BusDoc[], master = chain()) => ({ tracks, buses, master: { volume_db: 0, channels: "stereo", height: 72, effects: master } }) as unknown as SessionDoc;

describe("routing choices", () => {
  // voice -> mix; reverb -> mix; voice sends to reverb.
  const doc = session(
    [track("voice", { output: { type: "bus", bus_id: "mix" }, sends: [newSend("reverb", "x")] }), track("music")],
    [bus("mix"), bus("reverb", { output: { type: "bus", bus_id: "mix" } })],
  );

  it("follows outputs, sends and keys", () => {
    expect(reaches(doc, "voice", "mix")).toBe(true);
    expect(reaches(doc, "reverb", "mix")).toBe(true);
    expect(reaches(doc, "mix", "reverb")).toBe(false);
    expect(reaches(doc, "music", "mix")).toBe(false);
  });

  it("leaves out choices that would loop", () => {
    expect(busChoices(doc, "voice").map((b) => b.id)).toEqual(["mix", "reverb"]);
    // Mix feeding reverb would loop, and a bus never feeds itself.
    expect(busChoices(doc, "mix").map((b) => b.id)).toEqual([]);
    expect(busChoices(doc, "reverb").map((b) => b.id)).toEqual(["mix"]);
    // Music can listen to anything: nothing it feeds leads back to it.
    expect(keyChoices(doc, "music").map((k) => k.id)).toEqual(["voice", "mix", "reverb"]);
    // Voice feeds mix, so voice listening to mix would loop.
    expect(keyChoices(doc, "voice").map((k) => k.id)).toEqual(["music"]);
    expect(keyChoices(doc, MASTER_ID).length).toBe(4);
  });

  it("knows when the mix is routed", () => {
    expect(isRouted(doc)).toBe(true);
    expect(isRouted(session([track("a")], [bus("unused")]))).toBe(false);
    expect(isRouted(session([track("a")], [], { ...chain(), input_gain_db: 2 }))).toBe(true);
    expect(isRouted(session([track("a", { effects: chain("b") }), track("b")], []))).toBe(true);
    expect(isRouted(null)).toBe(false);
  });

  it("names outputs and edits sends", () => {
    expect(outputName(doc, { type: "bus", bus_id: "mix" })).toBe("mix");
    expect(outputName(doc, { type: "master" })).toBe("Master");
    expect(outputName(doc, { type: "bus", bus_id: "gone" })).toBe("Master");
    const s = [newSend("mix", "a"), newSend("reverb", "b")];
    expect(setSend(s, "b", { level_db: -6 })[1]!.level_db).toBe(-6);
    expect(removeSend(s, "a").map((x) => x.id)).toEqual(["b"]);
  });

  it("solos a bus exclusively when asked", () => {
    const d = session([track("a", { solo: true })], [bus("b"), bus("c", { solo: true })]);
    expect(busOp(d, "b", { solo: true }, false)).toEqual({ op: "set_bus", id: "b", changes: { solo: true } });
    expect(busOp(d, "b", { solo: true }, true)).toEqual({
      op: "batch",
      ops: [
        { op: "set_bus", id: "b", changes: { solo: true } },
        { op: "set_track", id: "a", changes: { solo: false } },
        { op: "set_bus", id: "c", changes: { solo: false } },
      ],
    });
  });

  it("solos and unsolos the tracks and buses feeding a bus", () => {
    // a -> sub -> b; c -> b; d sends only.
    const d = session(
      [track("a", { output: { type: "bus", bus_id: "sub" } }), track("c", { output: { type: "bus", bus_id: "b" } }), track("d", { sends: [newSend("b", "x")] }), track("e", { solo: true })],
      [bus("b"), bus("sub", { output: { type: "bus", bus_id: "b" } })],
    );
    expect(feeders(d, "b")).toEqual({ tracks: ["c", "a"], buses: ["sub"] });
    expect(busOp(d, "b", { solo: true }, false)).toEqual({
      op: "batch",
      ops: [
        { op: "set_bus", id: "b", changes: { solo: true } },
        { op: "set_track", id: "a", changes: { solo: true } },
        { op: "set_track", id: "c", changes: { solo: true } },
        { op: "set_bus", id: "sub", changes: { solo: true } },
      ],
    });
    // Exclusive: the feeders stay, the unrelated soloed track goes.
    const ex = busOp(d, "b", { solo: true }, true) as { ops: unknown[] };
    expect(ex.ops).toContainEqual({ op: "set_track", id: "e", changes: { solo: false } });
    expect(ex.ops).not.toContainEqual({ op: "set_track", id: "a", changes: { solo: false } });
    const on = session(
      d.tracks.map((t) => ({ ...t, solo: t.id !== "d" })),
      (d.buses ?? []).map((b) => ({ ...b, solo: true })),
    );
    expect(busOp(on, "b", { solo: false }, true)).toEqual({
      op: "batch",
      ops: [
        { op: "set_bus", id: "b", changes: { solo: false } },
        { op: "set_track", id: "a", changes: { solo: false } },
        { op: "set_track", id: "c", changes: { solo: false } },
        { op: "set_bus", id: "sub", changes: { solo: false } },
      ],
    });
  });
});

describe("sidechain sends", () => {
  it("lists effects a sound can drive, and points one at it", () => {
    const doc = session([track("guide"), track("dub", { effects: chain() }), track("other", { effects: chain("guide") })], [bus("mix")]);
    (doc.tracks[1]!.effects as EffectChainDoc).slots.push({ id: "va", effect: "vst3:va", enabled: true, params: {}, preset: null });
    const keyed = (e: string) => e === "vst3:va" || e === "altaudio.dynamics";
    const t = keyTargets(doc, "guide", keyed);
    expect(t.map((k) => [k.owner, k.slotId, k.on])).toEqual([
      ["dub", "va", false],
      ["other", "s", true],
    ]);
    const op = keyOp(doc, t[0]!, "guide");
    expect(op && op.op === "set_effects" && op.chain.slots[0]!.sidechain).toBe("guide");
    // Its own effects are not offered.
    expect(keyTargets(doc, "dub", keyed).map((k) => k.owner)).toEqual(["other"]);
  });
});
