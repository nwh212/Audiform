// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Theme files (`.audtheme`): the format, and the check every file passes
 * before it is used. A bad file gets a plain message, never an exception.
 * The format is described in docs/themes.md and schemas/theme.schema.json.
 */

export const THEME_FORMAT = "audiform-theme";
export const THEME_VERSION = 1;
/** The built-in default look (tokens.css). It has no file and no other theme may use this id. */
export const DEFAULT_THEME_ID = "dark";

export const POINTER_KINDS = ["text", "ew", "ns", "nwse", "nesw", "move", "grab", "grabbing", "busy"] as const;
export type PointerKind = (typeof POINTER_KINDS)[number];

export interface ThemePointer {
  /** Name of a picture in `assets`. */
  asset: string;
  /** Hot spot in pixels from the picture's top left corner. */
  hot: [number, number];
}

/** Flags for views that draw themselves (canvases) or need a hook the style sheet cannot give. */
export interface CanvasLook {
  /** Pixel look: views that can draw blocky (the Phase Wheel) do. */
  pixel?: boolean;
  /** Anything whose own style asks for the hand or the plain arrow gets the theme's arrow mark. */
  markCursor?: boolean;
  /** Glass look: translucent panels over a picture. For views that choose a backing. */
  glass?: boolean;
  /** Marble look: textured panels. For views that choose a backing. */
  marble?: boolean;
}

export interface ThemeFile {
  format: typeof THEME_FORMAT;
  version: number;
  id: string;
  name: string;
  base: "dark" | "light";
  description?: string;
  author?: string;
  secret?: boolean;
  css: string;
  assets: Record<string, string>;
  pointers?: Partial<Record<PointerKind, ThemePointer>>;
  canvasLook?: CanvasLook;
}

export type ThemeCheck = { ok: true; theme: ThemeFile } | { ok: false; error: string };

export const LIMITS = {
  id: 48,
  name: 80,
  description: 500,
  author: 100,
  css: 512 * 1024,
  assets: 200,
  assetBytes: 8 * 1024 * 1024,
  assetName: 64,
  hot: 128,
} as const;

const ID = /^[a-z0-9]+(-[a-z0-9]+)*$/;
const ASSET_NAME = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;
/** Pictures and fonts only: no scripts, no documents. */
const DATA_URI = /^data:(image\/(png|jpeg|webp|gif|avif|svg\+xml)|font\/(woff2|woff|ttf|otf));base64,[A-Za-z0-9+/=]+$/;
const FLAGS = ["pixel", "markCursor", "glass", "marble"] as const;

/** Every `url(asset:name)` in the css, by name. */
export function assetRefs(css: string): string[] {
  return [...css.matchAll(/url\(\s*["']?asset:([^"')\s]*)["']?\s*\)/g)].map((m) => m[1] ?? "");
}

/** Why the css is not allowed, or null. Nothing may reach the network. */
function badCss(css: string): string | null {
  if (css.length > LIMITS.css) return "The style sheet is too large.";
  if (/@import\b/i.test(css)) return "The style sheet uses @import. Themes cannot load other files.";
  if (/(^|[^\w-])(-webkit-)?image-set\s*\(|(^|[^\w-])src\s*\(/i.test(css)) return "The style sheet uses image-set() or src(). Use url(asset:name).";
  if (/<\/?style/i.test(css)) return "The style sheet contains a style tag.";
  for (const m of css.matchAll(/url\(\s*([^)]*?)\s*\)/gi)) {
    const target = (m[1] ?? "").replace(/^["']|["']$/g, "");
    if (target.startsWith("asset:") || target.startsWith("#")) continue;
    return `The style sheet loads "${target.slice(0, 40)}". Themes may only use url(asset:name) and url(#name).`;
  }
  return null;
}

function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function text(o: Record<string, unknown>, key: string, max: number, required: boolean): string | undefined | Error {
  const v = o[key];
  if (v === undefined || v === null) return required ? new Error(`"${key}" is missing.`) : undefined;
  if (typeof v !== "string") return new Error(`"${key}" must be text.`);
  const s = v.trim();
  if (required && !s) return new Error(`"${key}" is empty.`);
  if (s.length > max) return new Error(`"${key}" is too long (the limit is ${max} characters).`);
  return s;
}

/** Checks the text of a theme file. */
export function validateTheme(source: string): ThemeCheck {
  let raw: unknown;
  try {
    raw = JSON.parse(source);
  } catch {
    return { ok: false, error: "This is not a theme file (it is not valid JSON)." };
  }
  return validateThemeObject(raw);
}

/** Checks a theme that is already parsed. */
export function validateThemeObject(raw: unknown): ThemeCheck {
  const fail = (error: string): ThemeCheck => ({ ok: false, error });
  if (!isObject(raw)) return fail("This is not a theme file.");
  if (raw.format !== THEME_FORMAT) return fail(`This is not a theme file ("format" must be "${THEME_FORMAT}").`);
  if (typeof raw.version !== "number" || !Number.isInteger(raw.version) || raw.version < 1) return fail('"version" must be a whole number, 1 or more.');
  if (raw.version > THEME_VERSION) return fail(`This theme was made for a newer Audiform (format version ${raw.version}). Update Audiform to use it.`);

  const id = text(raw, "id", LIMITS.id, true);
  if (id instanceof Error) return fail(id.message);
  if (!id || !ID.test(id)) return fail('"id" must be lowercase letters, digits and single dashes, like "my-theme".');
  if (id === DEFAULT_THEME_ID) return fail(`"${DEFAULT_THEME_ID}" is the built-in look; a theme needs an id of its own.`);
  const name = text(raw, "name", LIMITS.name, true);
  if (name instanceof Error) return fail(name.message);
  if (raw.base !== "dark" && raw.base !== "light") return fail('"base" must be "dark" or "light".');
  const description = text(raw, "description", LIMITS.description, false);
  if (description instanceof Error) return fail(description.message);
  const author = text(raw, "author", LIMITS.author, false);
  if (author instanceof Error) return fail(author.message);
  if (raw.secret !== undefined && typeof raw.secret !== "boolean") return fail('"secret" must be true or false.');

  if (typeof raw.css !== "string") return fail('"css" is missing or is not text.');
  const css = badCss(raw.css);
  if (css) return fail(css);

  const assets: Record<string, string> = {};
  if (raw.assets !== undefined) {
    if (!isObject(raw.assets)) return fail('"assets" must be a list of named pictures.');
    const entries = Object.entries(raw.assets);
    if (entries.length > LIMITS.assets) return fail(`Too many assets (the limit is ${LIMITS.assets}).`);
    for (const [key, value] of entries) {
      if (key.length > LIMITS.assetName || !ASSET_NAME.test(key)) return fail(`The asset name "${key.slice(0, 20)}" is not allowed. Use letters, digits, dots, dashes and underscores.`);
      if (typeof value !== "string" || !DATA_URI.test(value)) return fail(`Asset "${key}" must be a base64 data: address of a picture or a font.`);
      if (value.length > LIMITS.assetBytes) return fail(`Asset "${key}" is too large (the limit is ${LIMITS.assetBytes / (1024 * 1024)} MB).`);
      assets[key] = value;
    }
  }
  for (const ref of assetRefs(raw.css)) {
    if (!(ref in assets)) return fail(`The style sheet uses a picture named "${ref}" that is not in "assets".`);
  }

  const pointers: Partial<Record<PointerKind, ThemePointer>> = {};
  if (raw.pointers !== undefined) {
    if (!isObject(raw.pointers)) return fail('"pointers" must be a list of pointer kinds.');
    for (const [kind, p] of Object.entries(raw.pointers)) {
      if (!(POINTER_KINDS as readonly string[]).includes(kind)) return fail(`"${kind.slice(0, 20)}" is not a pointer kind. The kinds are ${POINTER_KINDS.join(", ")}.`);
      if (!isObject(p) || typeof p.asset !== "string") return fail(`Pointer "${kind}" needs an "asset" name.`);
      if (!(p.asset in assets)) return fail(`Pointer "${kind}" uses a picture named "${p.asset.slice(0, 30)}" that is not in "assets".`);
      const hot = p.hot;
      const okHot =
        Array.isArray(hot) && hot.length === 2 && hot.every((n) => typeof n === "number" && Number.isInteger(n) && n >= 0 && n <= LIMITS.hot);
      if (!okHot) return fail(`Pointer "${kind}" needs "hot" as two whole numbers, like [8, 8].`);
      pointers[kind as PointerKind] = { asset: p.asset, hot: [(hot as number[])[0] as number, (hot as number[])[1] as number] };
    }
  }

  const look: CanvasLook = {};
  if (raw.canvasLook !== undefined) {
    if (!isObject(raw.canvasLook)) return fail('"canvasLook" must be a list of flags.');
    for (const flag of FLAGS) {
      const v = raw.canvasLook[flag];
      if (v === undefined) continue;
      if (typeof v !== "boolean") return fail(`canvasLook "${flag}" must be true or false.`);
      look[flag] = v;
    }
  }

  const theme: ThemeFile = {
    format: THEME_FORMAT,
    version: raw.version,
    id,
    name: name as string,
    base: raw.base,
    css: raw.css,
    assets,
  };
  if (description) theme.description = description;
  if (author) theme.author = author;
  if (raw.secret === true) theme.secret = true;
  if (Object.keys(pointers).length) theme.pointers = pointers;
  if (Object.keys(look).length) theme.canvasLook = look;
  return { ok: true, theme };
}
