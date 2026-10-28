// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { byCategory, sdrText, separatedTrack } from "./separation";
import type { SeparationModel } from "./core";
import type { TrackDoc } from "../stores/session";

const model = (id: string, category: string, sdr: number): SeparationModel => ({
  id,
  name: id,
  category,
  sdr,
  sdrNote: "",
  stems: [],
  targetName: "Vocals",
  restName: "Instrumental",
  author: "",
  page: "",
  bytes: 1,
  licence: "",
  installed: false,
});

describe("audio separation", () => {
  it("groups models by type, best score first", () => {
    const g = byCategory([model("a", "Vocals and instrumental", 10.98), model("b", "Drums", 9), model("c", "Vocals and instrumental", 11)]);
    expect(g.map(([c]) => c)).toEqual(["Vocals and instrumental", "Drums"]);
    expect(g[0]![1].map((m) => m.id)).toEqual(["c", "a"]);
    expect(sdrText(model("x", "", 11))).toBe("SDR 11.00 dB");
    expect(sdrText({ sdr: null, stems: [] })).toBe("SDR not published");
    expect(sdrText({ sdr: 11.3, stems: [["Vocals", 11.3], ["Bass", 14.62]] })).toBe("SDR vocals 11.3, bass 14.6 dB");
    // Unscored models list after scored ones.
    const g2 = byCategory([{ ...model("n", "Instruments", 0), sdr: null }, model("s", "Instruments", 5)]);
    expect(g2[0]![1].map((m) => m.id)).toEqual(["s", "n"]);
  });

  it("copies a track's settings onto a results track with new ids", () => {
    let n = 0;
    const id = () => `new${++n}`;
    const track = {
      id: "t1",
      name: "Song",
      role: "standard",
      channels: "stereo",
      color: "#123456",
      height: 80,
      volume_db: -3,
      pan: 20,
      muted: false,
      solo: true,
      record_armed: true,
      effects: { enabled: true, input_gain_db: 0, output_gain_db: 0, mix: 1, slots: [{ id: "s1", effect: "altaudio.parametric_eq", enabled: true, params: { g: 1 }, preset: null }] },
      envelopes: [{ id: "e1", target: { type: "effect_param", slot_id: "s1", param: "g" }, enabled: true, points: [] }],
      sends: [{ id: "d1", bus_id: "b1", level_db: -6, pan: 0, pre_fader: false, enabled: true }],
      clips: [
        { id: "c1", name: "Take", media_id: "m1", start: 100, source_in: 10, source_out: 500 },
        { id: "c2", name: "Other", media_id: "m2", start: 900, source_in: 0, source_out: 50 },
      ],
    } as unknown as TrackDoc;
    const out = separatedTrack(track, [track.clips[0]!], (m) => (m === "m1" ? { id: "r1", length: 1000 } : undefined), "Song Vocals", id);
    expect(out.name).toBe("Song Vocals");
    expect(out.id).not.toBe("t1");
    expect([out.volume_db, out.pan, out.color]).toEqual([-3, 20, "#123456"]);
    expect([out.solo, out.record_armed]).toEqual([false, false]);
    const slot = out.effects!.slots[0]!;
    expect(slot.id).not.toBe("s1");
    expect(slot.params).toEqual({ g: 1 });
    const env = out.envelopes![0]!;
    expect(env.id).not.toBe("e1");
    expect(env.target).toEqual({ type: "effect_param", slot_id: slot.id, param: "g" });
    expect(out.sends![0]!.id).not.toBe("d1");
    expect(out.clips).toHaveLength(1);
    expect(out.clips[0]).toMatchObject({ name: "Take", media_id: "r1", start: 100, source_in: 10, source_out: 500 });
    expect(out.clips[0]!.id).not.toBe("c1");
    // The source track is untouched.
    expect(track.effects!.slots[0]!.id).toBe("s1");
  });
});
