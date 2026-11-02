// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Builds docs/theme-maker/index.html, the Audiform Theme Maker page, from
 * tools/theme-maker/template.html: the design tokens are read from
 * src/styles/tokens.css (names, values and the comments above them, as
 * groups and notes), and the smaller shipped themes go in as starting
 * points. `--check` only compares, for CI: it fails when the page is stale.
 *
 *   node tools/theme-maker/build.mjs
 *   node tools/theme-maker/build.mjs --check
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const template = readFileSync(join(root, "tools/theme-maker/template.html"), "utf8");
const tokensCss = readFileSync(join(root, "src/styles/tokens.css"), "utf8");

/** Shipped themes small enough to carry inside the page (no big pictures). */
const STARTER_IDS = ["grey95", "analog", "marigold", "mallpastel", "mallnight", "phosphor"];

/** Labels for tokens whose names do not read well on their own. */
const LABELS = {
  "surface-0": "Window background",
  "surface-1": "Panels",
  "surface-2": "Bars and raised panels",
  "surface-3": "Buttons and fields",
  "menu-bg": "Open menus",
  "border": "Borders",
  "border-strong": "Strong borders",
  "text": "Text",
  "text-dim": "Dim text",
  "text-faint": "Faint text",
  "accent": "Accent (selection, focus)",
  "accent-dim": "Dim accent",
  "playhead": "Playhead",
  "value": "Numbers and links",
  "gutter": "Gaps between panels",
  "danger": "Record and warnings",
  "clip-muted-filter": "Muted clip filter",
  "radius-field": "Field corners",
  "radius-menu": "Menu corners",
  "radius-dialog": "Dialog corners",
  "lane-a": "Lanes (odd tracks)",
  "lane-b": "Lanes (even tracks)",
  "lane-selected": "Selected track's lane",
  "track-gap": "Gap between tracks",
  "header-a": "Track headers (odd)",
  "header-b": "Track headers (even)",
  "clip-body-alpha": "Clip body tint (0 to 1)",
  "clip-wave-tone": "Waveform tone (-1 to 1)",
  "clip-gloss": "Clip shine (0 or 1)",
  "font-ui": "Interface font",
  "font-mono": "Numbers font",
  "font-size": "Text size",
  "font-size-small": "Small text size",
  "row-height": "List row height",
};

/** Group names from the comments in tokens.css, by the first words of the comment. */
function groupOf(comment) {
  const c = comment.toLowerCase();
  if (c.startsWith("surfaces")) return "Surfaces";
  if (c.startsWith("text")) return "Text";
  if (c.startsWith("accent")) return "Accent and marks";
  if (c.startsWith("corner")) return "Shapes";
  if (c.startsWith("multitrack lanes")) return "Tracks";
  if (c.startsWith("clip palette")) return "Clips";
  if (c.startsWith("type and density")) return "Type and spacing";
  return null;
}

/** Tokens: `--name: value;` lines of the `:root {}` block, with the comments above them. */
function parseTokens(css) {
  const block = css.match(/:root\s*\{([\s\S]*?)\n\}/);
  if (!block) throw new Error("tokens.css has no :root block");
  const tokens = [];
  let group = "Surfaces";
  let note = "";
  const lines = block[1].split("\n");
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i].trim();
    if (!line) continue;
    if (line.startsWith("/*")) {
      let text = line;
      while (!text.includes("*/") && i + 1 < lines.length) text += " " + lines[++i].trim();
      const comment = text.replace(/^\/\*\s*|\s*\*\/$/g, "").replace(/\s+/g, " ");
      const g = groupOf(comment);
      if (g) {
        group = g;
        note = "";
      } else {
        note = comment;
      }
      continue;
    }
    const m = line.match(/^--([a-z0-9-]+):\s*(.*)$/i);
    if (!m) continue;
    let value = m[2];
    // A value continued on the next lines (the font stack).
    while (!value.endsWith(";") && i + 1 < lines.length) value += " " + lines[++i].trim();
    value = value.replace(/;$/, "").trim();
    const name = m[1];
    if (name === "color-scheme") continue;
    tokens.push({ name, value, group, label: LABELS[name] ?? name, note });
    note = "";
  }
  return tokens;
}

const tokens = parseTokens(tokensCss);
// Not tokens users set by hand: spacing steps the layout depends on.
const shown = tokens.filter((t) => !t.name.startsWith("space-"));

const starters = [
  {
    name: "Dark (the built-in look)",
    theme: {
      format: "audiform-theme",
      version: 1,
      id: "dark",
      name: "Dark",
      base: "dark",
      css: "",
      assets: {},
    },
  },
];
for (const id of STARTER_IDS) {
  const path = join(root, "src/themes", `${id}.audtheme`);
  if (!existsSync(path)) continue;
  const theme = JSON.parse(readFileSync(path, "utf8"));
  starters.push({ name: theme.name, theme });
}

/** JSON that is safe inside a <script> element. */
const inline = (v) => JSON.stringify(v).replace(/</g, "\\u003c").replace(/\u2028/g, "\\u2028").replace(/\u2029/g, "\\u2029");

const page = template.replace("/*TOKENS*/[]", inline(shown)).replace("/*STARTERS*/[]", inline(starters));
const out = join(root, "docs/theme-maker/index.html");

if (process.argv.includes("--check")) {
  const current = existsSync(out) ? readFileSync(out, "utf8") : "";
  if (current !== page) {
    console.error("docs/theme-maker/index.html is out of date. Run: node tools/theme-maker/build.mjs");
    process.exit(1);
  }
  console.log("Theme Maker page is up to date.");
} else {
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, page);
  console.log(`Wrote ${out} (${shown.length} tokens, ${starters.length} starting points, ${Math.round(page.length / 1024)} KB).`);
}
