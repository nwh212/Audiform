// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { ampSummary } from "./amp";

describe("amp model summary", () => {
  it("names the capture and cabinet", () => {
    const data = JSON.stringify({
      model: { architecture: "WaveNet", sample_rate: 48000, metadata: { name: "Crunch", gear_make: "Example", gear_model: "Amp 50" } },
      model_name: "crunch-file",
      ir: { name: "4x12", rate: 48000, samples: new Array(4800).fill(0) },
    });
    const s = ampSummary(data);
    expect(s.model).toMatchObject({ name: "Crunch", kind: "WaveNet", rate: 48000, gear: "Example Amp 50" });
    expect(s.ir).toEqual({ name: "4x12", rate: 48000, ms: 100 });
    expect(s.problem).toBeNull();
  });

  it("says when a capture will not play", () => {
    expect(ampSummary(JSON.stringify({ model: { architecture: "Sequential" } })).problem).toMatch(/not supported/);
    expect(ampSummary(JSON.stringify({ model: { architecture: "SlimmableContainer" } })).problem).toBeNull();
    expect(ampSummary(null)).toEqual({ model: null, ir: null, problem: null });
  });
});
