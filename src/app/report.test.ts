// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { ISSUE_URL, reportText, reportUrl } from "./report";

describe("problem reports", () => {
  it("titles the report from its first line and leaves out empty sections", () => {
    const r = reportText({ what: "Playback stops\nafter a minute", steps: "", expected: "", details: "Audiform 0.6.1, Windows" });
    expect(r.title).toBe("Playback stops");
    expect(r.body).toContain("## What happened\n\nPlayback stops\nafter a minute");
    expect(r.body).not.toContain("Steps to see it");
    expect(r.body).toContain("Audiform 0.6.1, Windows");
  });
  it("fills in the issue page and keeps the address short enough", () => {
    const url = reportUrl({ what: "x".repeat(20_000), steps: "", expected: "", details: null });
    expect(url.startsWith(`${ISSUE_URL}?title=`)).toBe(true);
    expect(url.length).toBeLessThanOrEqual(7000);
    expect(decodeURIComponent(url)).toContain("Shortened to fit");
  });
});
