// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Keeps private reference names out of public files. Reads the list of
 * terms in `private/banned-terms.txt` (never committed) and fails if any
 * tracked text file contains one. Does nothing when the list is missing,
 * so CI and other contributors are not affected.
 */

import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const root = new URL("..", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
const listFile = join(root, "private", "banned-terms.txt");
if (!existsSync(listFile)) {
  console.log("Private term check skipped: no private/banned-terms.txt here.");
  process.exit(0);
}
const terms = readFileSync(listFile, "utf8")
  .split(/\r?\n/)
  .map((t) => t.trim().toLowerCase())
  .filter((t) => t && !t.startsWith("#"));

/** Whole words only, so a short term never matches inside a longer word. */
const patterns = terms.map((t) => new RegExp(`(^|[^a-z0-9])${t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}([^a-z0-9]|$)`));

const skip = new Set(["node_modules", "target", "dist", "private", "samples", "out", "portable", "gen", "vendor"]);
const exts = [".md", ".ts", ".svelte", ".rs", ".json", ".html", ".css", ".yml", ".toml"];
/** @type {string[]} */
const files = [];
/** @param {string} dir */
function walk(dir) {
  for (const name of readdirSync(dir)) {
    if (skip.has(name) || name.startsWith(".git") || name === "CLAUDE.local.md") continue;
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p);
    else if (exts.some((e) => name.endsWith(e)) && name !== "package-lock.json" && !/\.test\.ts$/.test(name)) files.push(p);
  }
}
walk(root);

/** @type {string[]} */
const problems = [];
for (const f of files) {
  const text = readFileSync(f, "utf8").toLowerCase();
  for (const re of patterns) {
    if (re.test(text)) problems.push(`${relative(root, f)}: contains a private term`);
  }
}
if (problems.length > 0) {
  console.error([...new Set(problems)].join("\n"));
  process.exit(1);
}
console.log("Private term check OK.");
