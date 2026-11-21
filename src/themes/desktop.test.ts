// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { DESKTOP_THEME_ID, desktopTheme, type DesktopColors } from "./desktop";
import { validateTheme } from "./validate";

const DARK: DesktopColors = {
  "Window.BackgroundNormal": [29, 38, 44],
  "Window.ForegroundNormal": [241, 236, 237],
  "Window.ForegroundInactive": [126, 138, 146],
  "Window.ForegroundNegative": [173, 52, 49],
  "View.BackgroundNormal": [21, 31, 37],
  "View.BackgroundAlternate": [24, 33, 39],
  "View.ForegroundLink": [61, 174, 233],
  "Button.BackgroundNormal": [48, 55, 60],
  "Selection.BackgroundNormal": [51, 130, 171],
  "General.AccentColor": [61, 174, 233],
};
const LIGHT: DesktopColors = {
  "Window.BackgroundNormal": [239, 240, 241],
  "Window.ForegroundNormal": [35, 38, 41],
};

const token = (css: string, name: string) => new RegExp(`${name}: (#[0-9a-f]{6});`).exec(css)?.[1];

describe("Desktop Colors theme", () => {
  it("needs the window colors", () => {
    expect(desktopTheme({})).toBeNull();
    expect(desktopTheme({ "Window.BackgroundNormal": [0, 0, 0] })).toBeNull();
  });

  it("takes the scheme's colors and is a theme Audiform accepts", () => {
    const t = desktopTheme(DARK)!;
    expect(t.id).toBe(DESKTOP_THEME_ID);
    expect(t.base).toBe("dark");
    expect(token(t.css, "--surface-1")).toBe("#1d262c");
    expect(token(t.css, "--text")).toBe("#f1eced");
    expect(token(t.css, "--text-faint")).toBe("#7e8a92");
    expect(token(t.css, "--lane-a")).toBe("#151f25");
    expect(token(t.css, "--accent")).toBe("#3daee9");
    expect(token(t.css, "--danger")).toBe("#ad3431");
    expect(validateTheme(JSON.stringify(t)).ok).toBe(true);
  });

  it("starts from the light look for a light scheme and fills what is missing", () => {
    const t = desktopTheme(LIGHT)!;
    expect(t.base).toBe("light");
    expect(token(t.css, "--lane-a")).toBe("#eff0f1");
    expect(token(t.css, "--accent")).toBe("#3daee9");
    expect(validateTheme(JSON.stringify(t)).ok).toBe(true);
  });
});
