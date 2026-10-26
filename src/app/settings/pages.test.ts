// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS } from "../../stores/settings";
import { RECORD_DEFAULTS } from "../recording";
import { cleanDraft, LATER_PAGES, PAGES, sizeText } from "./pages";

describe("settings pages", () => {
  it("cleans a draft before it is applied", () => {
    const d = cleanDraft({
      app: { ...DEFAULT_SETTINGS, zoomStep: -3 },
      editor: { followPlayhead: true, autoCrossfade: false, timeFormat: "bogus" as never },
      record: { ...RECORD_DEFAULTS, offsetMs: 5000, prerollSec: Number.NaN },
      loudness: { region: "uk", target: "podcast-mono" },
    });
    expect(d.app.zoomStep).toBe(5);
    expect(d.editor.timeFormat).toBe("time");
    expect(d.record.offsetMs).toBe(1000);
    expect(d.record.prerollSec).toBe(2);
    expect(d.loudness.region).toBe("uk");
  });

  it("lists every page once and marks the later ones", () => {
    expect(new Set(PAGES).size).toBe(PAGES.length);
    for (const p of Object.keys(LATER_PAGES)) expect(PAGES).toContain(p);
    expect(sizeText(512)).toBe("512 B");
    expect(sizeText(1536)).toBe("1.5 KB");
    expect(sizeText(5 * 1024 * 1024 * 1024)).toBe("5.0 GB");
  });
});
