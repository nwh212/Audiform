// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { formatDecimal, samplesToSeconds } from "./time";

describe("samplesToSeconds", () => {
  it("converts at 48 kHz", () => {
    expect(samplesToSeconds(96_000, 48_000)).toBe(2);
  });

  it("rejects a zero sample rate", () => {
    expect(() => samplesToSeconds(1, 0)).toThrow(RangeError);
  });
});

describe("formatDecimal", () => {
  it("formats zero", () => {
    expect(formatDecimal(0, 48_000)).toBe("0:00.000");
  });

  it("formats minutes and milliseconds", () => {
    expect(formatDecimal(48_000 * 65.25, 48_000)).toBe("1:05.250");
  });

  it("adds hours past sixty minutes", () => {
    expect(formatDecimal(44_100 * 3723.5, 44_100)).toBe("1:02:03.500");
  });

  it("rolls rounding up into the next second", () => {
    expect(formatDecimal(48_000 * 59.9996, 48_000)).toBe("1:00.000");
  });

  it("keeps the sign for negative positions", () => {
    expect(formatDecimal(-48_000, 48_000)).toBe("-0:01.000");
  });
});
