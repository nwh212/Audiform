// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { EnvelopePointDoc } from "../../stores/session";
import { silenceSpan } from "./silence-span";

const p = (position: number, value: number): EnvelopePointDoc => ({ position, value, curve: "linear" });

describe("silenceSpan", () => {
  it("dips a flat clip with a ramp on each side", () => {
    expect(silenceSpan([], 1000, 400, 600, 10)).toEqual([p(390, 0), p(400, -60), p(600, -60), p(610, 0)]);
  });

  it("drops the points inside and keeps the ones outside", () => {
    const pts = [p(0, -6), p(300, -6), p(500, 3), p(900, 3)];
    const out = silenceSpan(pts, 1000, 400, 600, 10)!;
    expect(out.map((x) => x.position)).toEqual([0, 300, 390, 400, 600, 610, 900]);
    expect(out.find((x) => x.position === 390)!.value).toBeCloseTo(-6 + (3 + 6) * (90 / 200), 5);
    expect(out.find((x) => x.position === 400)!.value).toBe(-60);
  });

  it("has no ramp where the span touches the clip edge", () => {
    expect(silenceSpan([], 1000, -50, 200, 10)).toEqual([p(0, -60), p(200, -60), p(210, 0)]);
    expect(silenceSpan([], 1000, 800, 5000, 10)).toEqual([p(790, 0), p(800, -60), p(1000, -60)]);
  });

  it("returns null when the span misses the clip", () => {
    expect(silenceSpan([], 1000, 1000, 1200, 10)).toBeNull();
    expect(silenceSpan([], 1000, -300, 0, 10)).toBeNull();
  });

  it("keeps positions strictly increasing", () => {
    const out = silenceSpan([p(5, 0), p(395, 0)], 1000, 400, 600, 10)!;
    for (let i = 1; i < out.length; i++) expect(out[i]!.position).toBeGreaterThan(out[i - 1]!.position);
  });
});
