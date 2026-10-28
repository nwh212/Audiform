// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

// @ts-nocheck (a one-off tool; it is not part of the build)

// One-off conversion of the old hard-coded themes into `.audtheme` files.
//
// The old theme style sheets (src/styles/theme-*.css) and their pictures
// (src/assets/themes/*) were deleted when the themes became files. This
// script reads them back from the git revision that still has them, splits
// each style sheet per theme, turns every picture the css uses into a data
// URI in the theme's `assets`, and writes src/themes/<id>.audtheme.
//
// Run with: node scripts/pack-themes.mjs [revision]
// The default revision is the last commit that still had the old files.

import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, extname, join, basename } from "node:path";
import { fileURLToPath } from "node:url";
import postcss from "postcss";

const ROOT = fileURLToPath(new URL("..", import.meta.url));
const REV = process.argv[2] ?? "fb8b53b";
const OUT = join(ROOT, "src", "themes");
const AUTHOR = "The Audiform contributors";

const MIME = {
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".webp": "image/webp",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
  ".woff2": "font/woff2",
};

/** Reads a file as it was at REV. */
function gitShow(path) {
  return execFileSync("git", ["show", `${REV}:${path}`], {
    cwd: ROOT,
    maxBuffer: 64 * 1024 * 1024,
  });
}

/** The pointer kinds, and which asset files hold them. */
const GLASS = {
  files: { text: "cursor-text", ew: "cursor-ew", ns: "cursor-ns", nwse: "cursor-nwse", nesw: "cursor-nesw", grab: "cursor-grab", grabbing: "cursor-grabbing" },
  hot: () => [10, 10],
};
const PHOSPHOR = {
  files: { text: "phosphor-text", ew: "phosphor-ew", ns: "phosphor-ns", nwse: "phosphor-nwse", nesw: "phosphor-nesw", grab: "phosphor-grab", grabbing: "phosphor-grabbing" },
  // Pixel art at 2 screen pixels per cell, so each picture has its own middle.
  hot: (k) => ({ text: [7, 10], ew: [11, 5], ns: [5, 11], nwse: [9, 9], nesw: [9, 9], grab: [10, 9], grabbing: [10, 7] })[k],
};
const MALL = {
  files: { text: "mallpastel-text", ew: "mallpastel-ew", ns: "mallpastel-ns", nwse: "mallpastel-nwse", nesw: "mallpastel-nesw", grab: "mallpastel-grab", grabbing: "mallpastel-grabbing", busy: "mallpastel-busy" },
  hot: () => [8, 8],
};

/** Every theme: where its css lives, and what the code used to know about it by name. */
const THEMES = [
  { id: "meadow", file: "theme-aero.css", name: "Meadow Aero", base: "light", pointers: GLASS, look: { markCursor: true }, description: "Clear frosted glass over a grassy hill under a soft blue sky." },
  { id: "aqua", file: "theme-aero.css", name: "Aqua Aero", base: "light", pointers: GLASS, look: { markCursor: true }, description: "Clear frosted glass over sunlit water with light rays and rising bubbles." },
  { id: "moonlight", file: "theme-aero.css", name: "Moonlight Aero", base: "dark", pointers: GLASS, look: { markCursor: true }, description: "Smoky glass over a moonlit field under a sky full of stars." },
  { id: "prism", file: "theme-aero.css", name: "Prism", base: "dark", pointers: GLASS, look: { markCursor: true }, description: "Dark glass over glowing ribbons in every color of the Audiform mark." },
  { id: "phosphor", file: "theme-phosphor.css", name: "Phosphor", base: "dark", pointers: PHOSPHOR, look: { pixel: true, markCursor: true }, description: "An early studio computer: green screens in a beige case." },
  { id: "analog", file: "theme-analog.css", name: "Analog", base: "dark", pointers: null, look: {}, description: "A walnut and brushed metal studio desk." },
  { id: "mallpastel", file: "theme-mallpastel.css", name: "Mall Pastel '95 (モール・パステル '95)", base: "light", pointers: MALL, look: { markCursor: true }, description: "A pastel shopping plaza with marble columns." },
  { id: "mallnight", file: "theme-mallpastel.css", name: "Mall Pastel '95 Night (モール・パステル '95 ナイト)", base: "dark", pointers: MALL, look: { markCursor: true }, description: "The same plaza after closing." },
];

const IS_PREFIX = /^:root:is\(\s*((?:\[data-theme="[a-z0-9-]+"\]\s*,?\s*)+)\)/;
const ONE_PREFIX = /^:root\[data-theme="([a-z0-9-]+)"\]/;

/** For one selector: its theme ids and what follows the `:root` part, or null. */
function splitSelector(sel) {
  const s = sel.trim();
  let m = IS_PREFIX.exec(s);
  if (m) return { ids: [...m[1].matchAll(/data-theme="([a-z0-9-]+)"/g)].map((x) => x[1]), rest: s.slice(m[0].length) };
  m = ONE_PREFIX.exec(s);
  if (m) return { ids: [m[1]], rest: s.slice(m[0].length) };
  return null;
}

/** The part of a style sheet that applies to one theme, written for `:root[data-theme="id"]`. */
function cssFor(source, id) {
  const root = postcss.parse(source);
  root.walkComments((c) => c.remove());
  root.walkRules((rule) => {
    const keep = [];
    for (const sel of rule.selectors) {
      const parts = splitSelector(sel);
      if (!parts) throw new Error(`Selector without a theme: ${sel}`);
      if (parts.ids.includes(id)) keep.push(`:root[data-theme="${id}"]${parts.rest}`);
    }
    if (keep.length === 0) rule.remove();
    else rule.selectors = keep;
  });
  root.walkAtRules((a) => {
    if (a.parent === root && a.name !== "charset") throw new Error(`Unhandled at-rule @${a.name}`);
  });
  root.first && (root.first.raws.before = "");
  return root.toString().replace(/\n{3,}/g, "\n\n").trim() + "\n";
}

/** Swaps `url("../assets/themes/x.svg")` for `url(asset:x)` and notes the file. */
function useAssets(css, used) {
  return css.replace(/url\(\s*(["']?)\.\.\/assets\/themes\/([\w.-]+)\1\s*\)/g, (_all, _q, file) => {
    used.add(file);
    return `url(asset:${basename(file, extname(file))})`;
  });
}

function dataUri(file) {
  const mime = MIME[extname(file)];
  if (!mime) throw new Error(`Unknown picture type: ${file}`);
  return `data:${mime};base64,${gitShow(`src/assets/themes/${file}`).toString("base64")}`;
}

/** The file in src/assets/themes with this base name. */
function fileOf(name) {
  const hit = Object.keys(MIME)
    .map((e) => name + e)
    .find((f) => {
      try {
        gitShow(`src/assets/themes/${f}`);
        return true;
      } catch {
        return false;
      }
    });
  if (!hit) throw new Error(`No picture named ${name}`);
  return hit;
}

mkdirSync(OUT, { recursive: true });
const sheets = new Map();
for (const t of THEMES) {
  if (!sheets.has(t.file)) sheets.set(t.file, gitShow(`src/styles/${t.file}`).toString("utf8"));
  const used = new Set();
  const css = useAssets(cssFor(sheets.get(t.file), t.id), used);
  const pointers = {};
  if (t.pointers) {
    for (const [kind, name] of Object.entries(t.pointers.files)) {
      used.add(fileOf(name));
      pointers[kind] = { asset: name, hot: t.pointers.hot(kind) };
    }
  }
  const assets = {};
  for (const file of [...used].sort()) assets[basename(file, extname(file))] = dataUri(file);
  const theme = {
    format: "audiform-theme",
    version: 1,
    id: t.id,
    name: t.name,
    base: t.base,
    description: t.description,
    author: AUTHOR,
    canvasLook: t.look,
    ...(t.pointers ? { pointers } : {}),
    css,
    assets,
  };
  const out = join(OUT, `${t.id}.audtheme`);
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, JSON.stringify(theme, null, 2) + "\n");
  const kb = (JSON.stringify(theme).length / 1024).toFixed(0);
  console.log(`${t.id}: ${kb} KB, ${Object.keys(assets).length} assets`);
}
