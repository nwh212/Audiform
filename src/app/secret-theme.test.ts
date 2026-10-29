// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS } from "../stores/settings";
import { SECRET_THEME_ID, forgetSecretTheme, isSecretPhrase, tryUnlockSecretTheme } from "./secret-theme";
import { settings } from "../stores/settings";

describe("secret theme", () => {
  it("knows the phrase, in any case, and nothing else", async () => {
    expect(await isSecretPhrase("parissmith")).toBe(true);
    expect(await isSecretPhrase("  ParisSmith ")).toBe(true);
    expect(await isSecretPhrase("paris")).toBe(false);
    expect(await isSecretPhrase("1:02.500")).toBe(false);
    expect(await isSecretPhrase("")).toBe(false);
  });

  it("unlocks and switches to the theme", async () => {
    settings.set({ ...DEFAULT_SETTINGS, theme: "dark", unlockedThemes: [] });
    expect(await tryUnlockSecretTheme("nope")).toBe(false);
    expect(settings.current.theme).toBe("dark");
    expect(await tryUnlockSecretTheme("parissmith")).toBe(true);
    expect(settings.current.theme).toBe(SECRET_THEME_ID);
    expect(settings.current.unlockedThemes).toEqual([SECRET_THEME_ID]);
  });

  it("forgets the theme once another one is in use", () => {
    expect(forgetSecretTheme({ ...DEFAULT_SETTINGS, theme: SECRET_THEME_ID, unlockedThemes: [SECRET_THEME_ID] })).toBeNull();
    const left = forgetSecretTheme({ ...DEFAULT_SETTINGS, theme: "meadow", unlockedThemes: [SECRET_THEME_ID, "other"] });
    expect(left?.unlockedThemes).toEqual(["other"]);
    expect(forgetSecretTheme({ ...DEFAULT_SETTINGS, theme: "meadow", unlockedThemes: [] })).toBeNull();
  });
});

describe("secret theme file", () => {
  it("is listed only once unlocked, under a plain name", async () => {
    const { listThemes } = await import("../themes/loader");
    const hidden = await listThemes([]);
    expect(hidden.themes.some((t) => t.id === SECRET_THEME_ID)).toBe(false);
    expect(hidden.problems).toEqual([]);
    const shown = await listThemes([SECRET_THEME_ID]);
    expect(shown.themes.find((t) => t.id === SECRET_THEME_ID)?.name).toBe("Secret Theme");
  });
});
