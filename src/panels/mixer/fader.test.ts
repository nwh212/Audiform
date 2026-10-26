// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { dbToFader, faderToDb, formatDb, formatPan } from "./fader";

describe("fader law", () => {
  it("maps the ends and the knee", () => {
    expect(dbToFader(12)).toBe(1);
    expect(dbToFader(-60)).toBe(0);
    expect(dbToFader(-24)).toBeCloseTo(0.3);
    expect(faderToDb(0)).toBe(-96);
    expect(faderToDb(1)).toBe(12);
  });

  it("round-trips levels to a tenth of a dB", () => {
    for (const db of [-48, -24, -12.3, -6, 0, 3.5, 12]) {
      expect(faderToDb(dbToFader(db))).toBeCloseTo(db, 1);
    }
  });

  it("formats values", () => {
    expect(formatDb(-96)).toBe("-inf");
    expect(formatDb(3)).toBe("+3.0");
    expect(formatDb(0)).toBe("0.0");
    expect(formatPan(-20)).toBe("L20");
    expect(formatPan(0.2)).toBe("C");
  });
});
