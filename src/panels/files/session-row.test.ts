// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { sessionDuration } from "./session-row";

describe("sessionDuration", () => {
  it("formats minutes and hours", () => {
    expect(sessionDuration(0)).toBe("0:00.000");
    expect(sessionDuration(439.874)).toBe("7:19.874");
    expect(sessionDuration(3_725.5)).toBe("1:02:05.500");
  });
});
