// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS } from "../stores/settings";
import { EXTRA_THEME_ID, relock, isPhrase, tryUnlock } from "./unlock";
import { settings } from "../stores/settings";

describe("unlock", () => {
  it("reads the phrase in any case, and nothing else", async () => {
    expect(await isPhrase("parissmith")).toBe(true);
    expect(await isPhrase("  ParisSmith ")).toBe(true);
    expect(await isPhrase("paris")).toBe(false);
    expect(await isPhrase("1:02.500")).toBe(false);
    expect(await isPhrase("")).toBe(false);
  });

  it("unlocks and switches", async () => {
    settings.set({ ...DEFAULT_SETTINGS, theme: "dark", unlockedThemes: [] });
    expect(await tryUnlock("nope")).toBe(false);
    expect(settings.current.theme).toBe("dark");
    expect(await tryUnlock("parissmith")).toBe(true);
    expect(settings.current.theme).toBe(EXTRA_THEME_ID);
    expect(settings.current.unlockedThemes).toEqual([EXTRA_THEME_ID]);
  });

  it("relocks once another theme is in use", () => {
    expect(relock({ ...DEFAULT_SETTINGS, theme: EXTRA_THEME_ID, unlockedThemes: [EXTRA_THEME_ID] })).toBeNull();
    const left = relock({ ...DEFAULT_SETTINGS, theme: "meadow", unlockedThemes: [EXTRA_THEME_ID, "other"] });
    expect(left?.unlockedThemes).toEqual(["other"]);
    expect(relock({ ...DEFAULT_SETTINGS, theme: "meadow", unlockedThemes: [] })).toBeNull();
  });
});

describe("extra theme file", () => {
  it("is never listed, and can still be used once unlocked", async () => {
    const { listThemes, findTheme } = await import("../themes/loader");
    const hidden = await listThemes([]);
    expect(hidden.themes.some((t) => t.id === EXTRA_THEME_ID)).toBe(false);
    expect(hidden.problems).toEqual([]);
    const shown = await listThemes([EXTRA_THEME_ID]);
    expect(shown.themes.some((t) => t.id === EXTRA_THEME_ID)).toBe(false);
    expect(shown.secretIds).toContain(EXTRA_THEME_ID);
    expect(await findTheme(EXTRA_THEME_ID, [])).toBeNull();
    expect((await findTheme(EXTRA_THEME_ID, [EXTRA_THEME_ID]))?.id).toBe(EXTRA_THEME_ID);
  });
});
