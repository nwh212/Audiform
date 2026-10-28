// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Finds, checks and applies themes. Built-in themes are `.audtheme` files
 * in src/themes, loaded one at a time when they are needed. Users add
 * their own by dropping files in the Themes folder in the app config
 * folder. A theme that fails the check is left out with a plain message;
 * nothing here throws at the caller.
 */

import { DEFAULT_THEME_ID, validateTheme, type CanvasLook, type PointerKind, type ThemeFile } from "./validate";
import { setThemePointers, type ResolvedPointers } from "../app/theme-cursor";
import { themesList, themesRead } from "../app/core";

export interface ThemeEntry extends ThemeFile {
  /** Shipped with Audiform, as opposed to found in the Themes folder. */
  builtin: boolean;
  /** The file it came from. */
  file: string;
}

export interface ThemeProblem {
  file: string;
  message: string;
}

export interface ThemeCatalog {
  /** Built-in themes first, then the user's, each in name order. Secret ones not unlocked are left out. */
  themes: ThemeEntry[];
  /** Files that could not be used, with the reason. */
  problems: ThemeProblem[];
}

/** Where theme files come from besides the ones shipped. Replaceable in tests. */
export interface ThemeSource {
  /** The text of every built-in file, by file name. Loaded only when called. */
  builtin: () => Record<string, () => Promise<string>>;
  userList: () => Promise<{ fileName: string; size: number }[]>;
  userRead: (fileName: string) => Promise<string>;
}

const STYLE_ID = "audiform-theme";

/** Built-in files, as text, loaded on demand. */
const BUILTIN_FILES = import.meta.glob<string>("./*.audtheme", { query: "?raw", import: "default" });

function fileBase(path: string): string {
  return path.replace(/^.*\//, "");
}

let source: ThemeSource = {
  builtin: () => BUILTIN_FILES,
  userList: themesList,
  userRead: themesRead,
};

/** For tests: use fake files. Pass null to go back to the real ones. */
export function useThemeSource(s: ThemeSource | null): void {
  source = s ?? { builtin: () => BUILTIN_FILES, userList: themesList, userRead: themesRead };
  userCache = null;
  builtinCache.clear();
}

const builtinCache = new Map<string, ThemeEntry | ThemeProblem>();
let userCache: Promise<{ themes: ThemeEntry[]; problems: ThemeProblem[] }> | null = null;

function isProblem(v: ThemeEntry | ThemeProblem): v is ThemeProblem {
  return "message" in v;
}

async function loadBuiltin(path: string, read: () => Promise<string>): Promise<ThemeEntry | ThemeProblem> {
  const file = fileBase(path);
  const cached = builtinCache.get(file);
  if (cached) return cached;
  let result: ThemeEntry | ThemeProblem;
  try {
    const check = validateTheme(await read());
    if (!check.ok) result = { file, message: check.error };
    else if (`${check.theme.id}.audtheme` !== file) result = { file, message: `The file name must match the id "${check.theme.id}".` };
    else result = { ...check.theme, builtin: true, file };
  } catch (e) {
    result = { file, message: `Could not read the file: ${e instanceof Error ? e.message : String(e)}` };
  }
  builtinCache.set(file, result);
  return result;
}

async function loadUser(s: ThemeSource, builtinIds: Set<string>): Promise<{ themes: ThemeEntry[]; problems: ThemeProblem[] }> {
  const themes: ThemeEntry[] = [];
  const problems: ThemeProblem[] = [];
  let files: { fileName: string; size: number }[] = [];
  try {
    files = await s.userList();
  } catch {
    // No Themes folder to read (for example outside the app): no user themes.
    return { themes, problems };
  }
  const seen = new Set(builtinIds);
  for (const f of files) {
    try {
      const check = validateTheme(await s.userRead(f.fileName));
      if (!check.ok) problems.push({ file: f.fileName, message: check.error });
      else if (seen.has(check.theme.id)) problems.push({ file: f.fileName, message: `The id "${check.theme.id}" is already used by another theme.` });
      else {
        seen.add(check.theme.id);
        themes.push({ ...check.theme, builtin: false, file: f.fileName });
      }
    } catch (e) {
      problems.push({ file: f.fileName, message: `Could not read the file: ${e instanceof Error ? e.message : String(e)}` });
    }
  }
  return { themes, problems };
}

/** Whether a theme may be listed and used: secret ones only after they are unlocked. */
export function isAvailable(theme: Pick<ThemeFile, "id" | "secret">, unlocked: readonly string[]): boolean {
  return !theme.secret || unlocked.includes(theme.id);
}

function byName(a: ThemeEntry, b: ThemeEntry): number {
  return a.name.localeCompare(b.name);
}

/** The id of a built-in file: its file name. */
function builtinIdOf(path: string): string {
  return fileBase(path).replace(/\.audtheme$/, "");
}

/** Reads every built-in and user theme file and lists the ones that may be used. Pass `refresh` to look at the Themes folder again. */
export async function listThemes(unlocked: readonly string[], refresh = false): Promise<ThemeCatalog> {
  const s = source;
  if (refresh) userCache = null;
  const built = await Promise.all(Object.entries(s.builtin()).map(([path, read]) => loadBuiltin(path, read)));
  const builtinThemes = built.filter((b): b is ThemeEntry => !isProblem(b));
  const problems = built.filter(isProblem);
  userCache ??= loadUser(s, new Set(Object.keys(s.builtin()).map(builtinIdOf)));
  const user = await userCache;
  problems.push(...user.problems);
  return {
    themes: [...builtinThemes.filter((t) => isAvailable(t, unlocked)).sort(byName), ...user.themes.filter((t) => isAvailable(t, unlocked)).sort(byName)],
    problems,
  };
}

/**
 * The theme to use for a saved id, or null for the built-in default: an
 * unknown, broken or still locked theme is the default too. A built-in
 * theme loads on its own, so the common case reads one file.
 */
export async function findTheme(id: string, unlocked: readonly string[]): Promise<ThemeEntry | null> {
  if (id === DEFAULT_THEME_ID) return null;
  const s = source;
  const files = s.builtin();
  const path = Object.keys(files).find((p) => builtinIdOf(p) === id);
  if (path) {
    const t = await loadBuiltin(path, files[path] as () => Promise<string>);
    return !isProblem(t) && isAvailable(t, unlocked) ? t : null;
  }
  userCache ??= loadUser(s, new Set(Object.keys(files).map(builtinIdOf)));
  const user = await userCache;
  return user.themes.find((t) => t.id === id && isAvailable(t, unlocked)) ?? null;
}

/** The id to use for a saved one: itself when it can be used, else "dark". */
export async function resolveThemeId(id: string, unlocked: readonly string[]): Promise<string> {
  return (await findTheme(id, unlocked))?.id ?? DEFAULT_THEME_ID;
}

/** The css with every `url(asset:name)` turned into the picture itself. */
export function resolveCss(theme: Pick<ThemeFile, "css" | "assets" | "base" | "id">): string {
  const body = theme.css.replace(/url\(\s*["']?asset:([^"')\s]*)["']?\s*\)/g, (all, name: string) => {
    const uri = theme.assets[name];
    return uri ? `url("${uri}")` : all;
  });
  // Light themes say so first, so form controls and scroll bars start light; the theme can still override it.
  const scheme = theme.base === "light" ? `:root[data-theme="${theme.id}"] { color-scheme: light; }\n` : "";
  return scheme + body;
}

/** The pointer pictures of a theme, ready for theme-cursor. */
export function resolvePointers(theme: Pick<ThemeFile, "pointers" | "assets">): ResolvedPointers | null {
  if (!theme.pointers) return null;
  const out: ResolvedPointers = {};
  for (const [kind, p] of Object.entries(theme.pointers)) {
    const url = theme.assets[p.asset];
    if (url) out[kind as PointerKind] = { url, hot: p.hot };
  }
  return Object.keys(out).length ? out : null;
}

let currentLook: CanvasLook = {};
let currentId: string = DEFAULT_THEME_ID;

/** Flags of the theme in use, for views that draw themselves. */
export function themeLook(): CanvasLook {
  return currentLook;
}

/** Id of the theme in use ("dark" for the built-in look). */
export function activeThemeId(): string {
  return currentId;
}

/** Puts a theme on the page (or the built-in look, for null). */
export function installTheme(theme: ThemeEntry | null, doc: Document = document): void {
  const root = doc.documentElement;
  let style = doc.getElementById(STYLE_ID);
  if (!theme) {
    style?.remove();
    delete root.dataset.theme;
    currentLook = {};
    currentId = DEFAULT_THEME_ID;
    setThemePointers(null);
    return;
  }
  if (!style) {
    style = doc.createElement("style");
    style.id = STYLE_ID;
    doc.head.appendChild(style);
  }
  style.textContent = resolveCss(theme);
  root.dataset.theme = theme.id;
  currentLook = theme.canvasLook ?? {};
  currentId = theme.id;
  setThemePointers(resolvePointers(theme));
}

let latest = 0;

/**
 * Switches to the saved theme id. If a later call starts while this one
 * waits for its file, only the later one takes effect. Resolves to the id
 * that is in use.
 */
export async function applyThemeId(id: string, unlocked: readonly string[], doc: Document = document): Promise<string> {
  const mine = ++latest;
  let theme: ThemeEntry | null = null;
  try {
    theme = await findTheme(id, unlocked);
  } catch {
    theme = null;
  }
  if (mine !== latest) return currentId;
  installTheme(theme, doc);
  return currentId;
}
