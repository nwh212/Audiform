// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { dueForCheck, WEEK_MS } from "./updates";

describe("weekly update check", () => {
  it("runs only when switched on, at most once a week", () => {
    const now = 1_800_000_000_000;
    expect(dueForCheck(false, 0, now)).toBe(false);
    expect(dueForCheck(true, 0, now)).toBe(true);
    expect(dueForCheck(true, now - WEEK_MS + 1, now)).toBe(false);
    expect(dueForCheck(true, now - WEEK_MS, now)).toBe(true);
    // A clock moved backwards does not block it for ever.
    expect(dueForCheck(true, now + 1000, now)).toBe(true);
  });
});
