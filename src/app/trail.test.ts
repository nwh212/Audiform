// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { needsSteps, noteStep, recentSteps } from "./trail";

describe("trail", () => {
  it("keeps the last twelve steps, oldest first", () => {
    for (let i = 0; i < 15; i++) noteStep(`step ${i}`);
    const s = recentSteps();
    expect(s).toHaveLength(12);
    expect(s[0]).toMatch(/step 3$/);
    expect(s[11]).toMatch(/step 14$/);
  });
  it("picks out the update depth error", () => {
    expect(needsSteps("https://svelte.dev/e/effect_update_depth_exceeded")).toBe(true);
    expect(needsSteps("Could not open the file")).toBe(false);
  });
});
