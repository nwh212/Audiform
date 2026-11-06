// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import type { EffectSlotDoc } from "../../stores/session";
import { addPoint, curveAt, curveGroupShown, curveLinear, curvePoints, DYNAMICS_CURVE, pointCount, removePoint, RESET } from "./dynamics-curve";

const slot = (params: Record<string, number>): EffectSlotDoc => ({ id: "s", effect: DYNAMICS_CURVE, enabled: true, params }) as unknown as EffectSlotDoc;

describe("Dynamics Curve graph", () => {
  it("starts as a straight line", () => {
    const pts = curvePoints(slot({}));
    expect(pointCount(slot({}))).toBe(2);
    expect(pts.map((p) => [p.inDb, p.outDb])).toEqual([[-60, -60], [0, 0]]);
    for (const x of [-90, -60, -20, 0, 6]) expect(curveLinear(pts, x)).toBeCloseTo(x, 6);
  });

  it("squeezes above a corner and carries on at the last slope", () => {
    const pts = curvePoints(slot({ points: 3, p1_in: -60, p1_out: -60, p2_in: 0, p2_out: -10, p3_in: -20, p3_out: -20 }));
    expect(pts.map((p) => p.inDb)).toEqual([-60, -20, 0]);
    expect(curveLinear(pts, -10)).toBeCloseTo(-15, 6);
    expect(curveLinear(pts, 6)).toBeCloseTo(-10 + 6 * 0.5, 6);
    expect(curveLinear(pts, -80)).toBeCloseTo(-80, 6);
  });

  it("rounds a corner with the knee", () => {
    const pts = curvePoints(slot({ points: 3, p1_in: -60, p1_out: -60, p2_in: 0, p2_out: -10, p3_in: -20, p3_out: -20 }));
    expect(curveAt(pts, 0, -20)).toBeCloseTo(-20, 6);
    expect(curveAt(pts, 8, -20)).toBeLessThan(-20);
    expect(curveAt(pts, 8, -20)).toBeGreaterThan(-21);
    expect(curveAt(pts, 8, -50)).toBeCloseTo(-50, 6);
  });

  it("adds and removes points like the core", () => {
    const s = slot({});
    expect(addPoint(s, -30, -20)).toEqual({ points: 3, p3_in: -30, p3_out: -20 });
    expect(removePoint(s, 0)).toBeNull();
    const three = slot({ points: 3, p1_in: -60, p1_out: -60, p2_in: 0, p2_out: 0, p3_in: -30, p3_out: -20 });
    expect(removePoint(three, 0)).toEqual({ points: 2, p1_in: -30, p1_out: -20 });
    expect(removePoint(three, 2)).toEqual({ points: 2 });
    expect(addPoint(slot({ points: 8 }), -1, -1)).toBeNull();
    expect(RESET.points).toBe(2);
  });

  it("hides the groups of points not in use", () => {
    const s = slot({ points: 3 });
    expect(curveGroupShown(DYNAMICS_CURVE, s, "Point 3")).toBe(true);
    expect(curveGroupShown(DYNAMICS_CURVE, s, "Point 4")).toBe(false);
    expect(curveGroupShown("altaudio.limiter", s, "Point 4")).toBe(true);
    expect(curveGroupShown(DYNAMICS_CURVE, s, "Other")).toBe(true);
  });
});
