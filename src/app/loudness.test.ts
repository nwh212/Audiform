// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { evaluate, targetById, type LoudnessReport } from "./loudness";

const report = (integrated: number, truePeak: number, integratedMono = integrated - 3): LoudnessReport => ({
  integrated,
  integratedMono,
  range: 6,
  truePeak,
  samplePeak: truePeak - 0.2,
  seconds: 60,
  cancelled: false,
});

describe("loudness verdicts", () => {
  const stereo = targetById("podcast-stereo");

  it("passes a mix on target", () => {
    const v = evaluate(report(-16.4, -1.5), stereo);
    expect(v.loudness).toBe("pass");
    expect(v.peak).toBe("pass");
  });

  it("suggests a safe master change when there is headroom", () => {
    const v = evaluate(report(-20, -6), stereo);
    expect(v.loudness).toBe("fail");
    expect(v.gainToTarget).toBe(4);
    expect(v.gainIsSafe).toBe(true);
    expect(v.advice).toContain("Raise the Master by 4.0 dB");
  });

  it("warns when raising would clip, and flags hot peaks", () => {
    const v = evaluate(report(-20, -2), stereo);
    expect(v.gainIsSafe).toBe(false);
    expect(v.advice).toContain("limit");
    expect(evaluate(report(-16, 0.3), stereo).peak).toBe("fail");
  });

  it("uses the mono measurement for mono targets", () => {
    const v = evaluate(report(-16, -2, -19.2), targetById("podcast-mono"));
    expect(v.measured).toBe(-19.2);
    expect(v.loudness).toBe("pass");
  });

  it("handles silence", () => {
    expect(evaluate(report(-Infinity, -Infinity, -Infinity), stereo).advice).toContain("silent");
  });
});

import { BROADCAST_REGIONS, regionById, targetsFor } from "./loudness";

describe("broadcast regions", () => {
  it("defaults to the US standard", () => {
    const b = targetById("broadcast");
    expect(b.lufs).toBe(-24);
    expect(b.peak).toBe(-2);
  });
  it("follows the chosen region", () => {
    expect(targetById("broadcast", "europe").lufs).toBe(-23);
    expect(targetById("broadcast", "europe").peak).toBe(-1);
  });
  it("falls back to the US for an unknown region", () => {
    expect(regionById("mars").id).toBe("us");
  });
  it("only changes the broadcast target", () => {
    for (const r of BROADCAST_REGIONS) {
      const list = targetsFor(r.id);
      expect(list.find((t) => t.id === "podcast-stereo")!.lufs).toBe(-16);
      expect(list.find((t) => t.id === "broadcast")!.lufs).toBe(r.lufs);
    }
  });
});
