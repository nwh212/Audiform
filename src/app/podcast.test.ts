// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { LoudnessReport } from "./loudness";
import type { SessionDoc, TrackDoc } from "../stores/session";
import { balance, balanceItems, describeBalance } from "./speaker-balance";
import { HOST_PROFILES, profileChain, profileOp } from "./host-profiles";

const clip = (id: string, gain: number, audio_type?: string) => ({ id, gain_db: gain, audio_type });
const doc = {
  tracks: [
    { id: "h", name: "Host", muted: false, clips: [clip("h1", 0), clip("h2", -2)] },
    { id: "g", name: "Guest", muted: false, clips: [clip("g1", 0, "dialogue")] },
    { id: "m", name: "Music", muted: false, clips: [clip("m1", 0, "music")] },
    { id: "x", name: "Muted", muted: true, clips: [clip("x1", 0)] },
  ],
} as unknown as SessionDoc;
const report = (integrated: number): LoudnessReport => ({ integrated, integratedMono: integrated - 3, truePeak: -3 }) as unknown as LoudnessReport;

describe("speaker balance", () => {
  it("measures voice tracks only and brings them to the quietest", () => {
    const items = balanceItems(doc);
    expect(items.map((i) => i.key)).toEqual(["h", "g"]);
    const b = balance(doc, items, [report(-18), report(-24)]);
    expect(b.reference?.key).toBe("g");
    expect(b.changes).toEqual([
      { id: "h1", gain_db: -6 },
      { id: "h2", gain_db: -8 },
    ]);
    expect(describeBalance(b)).toBe("Balanced to Guest (-24.0 LUFS): Host -6.0 dB.");
  });

  it("leaves close voices and silent tracks alone", () => {
    const items = balanceItems(doc);
    expect(balance(doc, items, [report(-20), report(-20.3)]).changes).toEqual([]);
    const silent = balance(doc, items, [report(-20), report(Number.NEGATIVE_INFINITY)]);
    expect(silent.changes).toEqual([]);
    expect(describeBalance(silent)).toMatch(/at least two/);
  });
});

describe("host profiles", () => {
  let n = 0;
  const id = () => `s${n++}`;
  it("ends each chain with a gate set for the profile", () => {
    for (const p of HOST_PROFILES) {
      const c = profileChain(p, id);
      const gates = c.slots.filter((s) => s.effect === "altaudio.breath_gate");
      expect(gates).toHaveLength(1);
      expect(c.slots[c.slots.length - 1]!.params.threshold).toBe(p.gateDb);
    }
    const guest = profileChain(HOST_PROFILES[2]!, id);
    expect(guest.slots.find((s) => s.effect === "altaudio.parametric_eq")!.params.lp_on).toBe(1);
  });

  it("applies and clears as one edit", () => {
    const t = { id: "t" } as TrackDoc;
    const op = profileOp(t, HOST_PROFILES[2]!, id);
    expect(op).toMatchObject({ op: "batch", ops: [{ op: "set_track", changes: { host_profile_id: "remote-guest", role: "guest" } }, { op: "set_effects" }] });
    expect(profileOp(t, null, id)).toEqual({ op: "set_track", id: "t", changes: { host_profile_id: "", role: "standard" } });
  });
});
