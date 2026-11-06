// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { acxCheck } from "./delivery";
import type { LoudnessReport } from "./loudness";

const rep = (rms: number, samplePeak: number, noiseFloor: number | null, silentSeconds = 0): LoudnessReport => ({
  integrated: -20,
  integratedMono: -23,
  range: 5,
  truePeak: samplePeak,
  samplePeak,
  seconds: 60,
  cancelled: false,
  rms,
  noiseFloor,
  silentSeconds,
});

describe("audiobook delivery check", () => {
  it("passes narration inside all three limits", () => {
    const v = acxCheck(rep(-20, -4, -65));
    expect(v.pass).toBe(true);
    expect(v.rules.map((r) => r.grade)).toEqual(["pass", "pass", "pass"]);
    expect(v.advice[0]).toContain("192 kbps");
  });

  it("treats the band edges as passing and a dB outside as close", () => {
    expect(acxCheck(rep(-18, -3, -60)).pass).toBe(true);
    expect(acxCheck(rep(-23.5, -4, -65)).rules[0]!.grade).toBe("close");
    expect(acxCheck(rep(-25, -4, -65)).rules[0]!.grade).toBe("fail");
  });

  it("suggests a master gain when the peaks leave room", () => {
    const v = acxCheck(rep(-26, -12, -70));
    expect(v.gainToTarget).toBe(5.5);
    expect(v.gainIsSafe).toBe(true);
    expect(v.advice[0]).toContain("Raise the Master 5.5 dB");
  });

  it("sends the user to the limiter when raising would clip", () => {
    const v = acxCheck(rep(-26, -4, -70));
    expect(v.gainIsSafe).toBe(false);
    expect(v.advice[0]).toContain("limit");
  });

  it("flags hot peaks, a noisy floor and digital silence", () => {
    const v = acxCheck(rep(-20, -1, -50, 2));
    expect(v.pass).toBe(false);
    expect(v.advice.some((a) => a.includes("Peaks reach -1.0"))).toBe(true);
    expect(v.advice.some((a) => a.includes("room tone sits at -50.0"))).toBe(true);
    expect(v.advice.some((a) => a.includes("digital silence"))).toBe(true);
  });

  it("explains a file with no quiet gap", () => {
    const v = acxCheck(rep(-20, -4, null));
    expect(v.rules[2]!.grade).toBe("fail");
    expect(v.advice[0]).toContain("room tone");
  });

  it("handles silence", () => {
    const v = acxCheck(rep(-Infinity, -Infinity, null));
    expect(v.advice[0]).toContain("silent");
    expect(v.pass).toBe(false);
  });
});
