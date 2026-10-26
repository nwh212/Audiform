// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { PreviewMeter } from "./preview-meter";

describe("preview meter", () => {
  it("moves while a preview plays and drops when it stops", () => {
    const m = new PreviewMeter({ status: async () => ({ playing: false }) });
    let last = { playing: false, master: [{ level: 0 }, { level: 0 }] } as unknown as Parameters<Parameters<typeof m.subscribe>[0]>[0];
    const off = m.subscribe((s) => (last = s));
    m.apply({ playing: true, peak: [0.5, 0.25] });
    expect(last.playing).toBe(true);
    expect(last.master[0].level).toBe(0.5);
    expect(last.master[1].peak).toBe(0.25);
    m.apply({ playing: true, peak: [1.2, 0] });
    expect(last.master[0].over).toBe(true);
    m.apply({ playing: false });
    expect(last.master[0].level).toBe(0);
    expect(last.master[0].over).toBe(true);
    m.clearOver();
    expect(last.master[0].over).toBe(false);
    off();
  });
});
