// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * The Desktop Colors theme: made at run time from the desktop's own color
 * scheme (on Linux with KDE, the colors in `kdeglobals`, read by the core),
 * so the app matches the rest of the desktop and follows it when the scheme
 * changes. It is offered only where the core finds such colors.
 */

import type { ThemeFile } from "./validate";
import { THEME_FORMAT } from "./validate";

export const DESKTOP_THEME_ID = "desktop-colors";

type Rgb = readonly [number, number, number];
/** The scheme as the core sends it: "Window.BackgroundNormal" to [r, g, b]. */
export type DesktopColors = Record<string, Rgb>;

const hex = ([r, g, b]: Rgb) => `#${[r, g, b].map((v) => Math.round(v).toString(16).padStart(2, "0")).join("")}`;
/** `a` moved toward `b` by `t` (0 to 1). */
const mix = (a: Rgb, b: Rgb, t: number): Rgb => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
/** Relative brightness, 0 to 1. */
const light = ([r, g, b]: Rgb) => (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
const BLACK: Rgb = [0, 0, 0];
const WHITE: Rgb = [255, 255, 255];

/** The theme for a color scheme, or null when it lacks the colors a theme needs. */
export function desktopTheme(c: DesktopColors): ThemeFile | null {
  const win = c["Window.BackgroundNormal"];
  const text = c["Window.ForegroundNormal"];
  if (!win || !text) return null;
  const view = c["View.BackgroundNormal"] ?? win;
  const viewAlt = c["View.BackgroundAlternate"] ?? mix(view, text, 0.04);
  const button = c["Button.BackgroundNormal"] ?? mix(win, text, 0.08);
  const selection = c["Selection.BackgroundNormal"] ?? c["General.AccentColor"] ?? [61, 174, 233];
  const accent = c["General.AccentColor"] ?? c["View.DecorationFocus"] ?? selection;
  const dark = light(win) < 0.5;
  // Gaps between panels and tracks: a step deeper than the window, whichever way the scheme runs.
  const deep = mix(win, BLACK, dark ? 0.35 : 0.2);
  const lift = dark ? WHITE : BLACK;

  const tokens: Record<string, string> = {
    "--surface-0": hex(mix(win, BLACK, dark ? 0.15 : 0.05)),
    "--surface-1": hex(win),
    "--surface-2": hex(mix(win, lift, 0.05)),
    "--surface-3": hex(button),
    "--menu-bg": hex(mix(win, lift, 0.05)),
    "--border": hex(mix(win, text, 0.15)),
    "--border-strong": hex(mix(win, text, 0.28)),
    "--text": hex(text),
    "--text-dim": hex(mix(text, win, 0.3)),
    "--text-faint": hex(c["Window.ForegroundInactive"] ?? mix(text, win, 0.5)),
    "--accent": hex(accent),
    "--accent-dim": hex(mix(accent, win, 0.45)),
    "--value": hex(c["View.ForegroundLink"] ?? accent),
    "--gutter": hex(deep),
    "--danger": hex(c["Window.ForegroundNegative"] ?? [229, 83, 75]),
    "--lane-a": hex(view),
    "--lane-b": hex(viewAlt),
    "--lane-selected": hex(mix(view, selection, 0.3)),
    "--track-gap": hex(deep),
    "--header-a": hex(mix(win, lift, 0.03)),
    "--header-b": hex(mix(win, lift, 0.07)),
  };
  const body = Object.entries(tokens)
    .map(([k, v]) => `  ${k}: ${v};`)
    .join("\n");
  return {
    format: THEME_FORMAT,
    version: 1,
    id: DESKTOP_THEME_ID,
    name: "Desktop Colors",
    base: dark ? "dark" : "light",
    description: "The colors of your desktop's color scheme. Changes with it: switch the scheme and Audiform follows when you come back to its window.",
    css: `:root[data-theme="${DESKTOP_THEME_ID}"] {\n${body}\n}\n`,
    assets: {},
  };
}
