// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

// Fails if any source file is missing the GPLv3 header: the license line
// and a copyright line. Files from Altered Audio Editor keep The GeekCast
// Radio Network's copyright; files new in Audiform name their own authors.
// Run with: node scripts/check-headers.mjs

import { readdirSync, readFileSync, statSync } from "node:fs";
import { extname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL("..", import.meta.url));
const SCAN = ["src", "src-tauri/src", "src-tauri/tests", "scripts"];
const EXTENSIONS = new Set([".rs", ".ts", ".js", ".mjs", ".svelte", ".css"]);
const SKIP_DIRS = new Set(["node_modules", "target", "gen", "fixtures", "dist"]);
const REQUIRED = ["SPDX-License-Identifier: GPL-3.0-or-later", "Copyright (c) "];

/**
 * @param {string} dir
 * @returns {Generator<string>}
 */
function* walk(dir) {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) {
      if (!SKIP_DIRS.has(name)) yield* walk(path);
    } else if (EXTENSIONS.has(extname(name))) {
      yield path;
    }
  }
}

/** @type {string[]} */
const missing = [];
for (const top of SCAN) {
  for (const file of walk(join(ROOT, top))) {
    const head = readFileSync(file, "utf8").slice(0, 600);
    if (!REQUIRED.every((line) => head.includes(line))) {
      missing.push(relative(ROOT, file));
    }
  }
}

if (missing.length > 0) {
  console.error("Missing GPLv3 header in:\n  " + missing.join("\n  "));
  process.exit(1);
}
console.log("License headers OK.");
