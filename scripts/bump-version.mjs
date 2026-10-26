// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Bumps the app version in every file that carries it, so they never
 * disagree: package.json, package-lock.json, src-tauri/Cargo.toml,
 * src-tauri/Cargo.lock and src-tauri/tauri.conf.json.
 *
 *   npm run bump            0.1.1 -> 0.1.2  (each release)
 *   npm run bump -- minor   0.1.9 -> 0.2.0  (a phase lands)
 *   npm run bump -- 1.0.0   set it outright
 *
 * Files are written as UTF-8 without a byte order mark, which the
 * installer build needs.
 */

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
/** @param {string} p */
const read = (p) => readFileSync(join(root, p), "utf8").replace(/^﻿/, "");
/** @param {string} p @param {string} s */
const write = (p, s) => writeFileSync(join(root, p), s, { encoding: "utf8" });

const pkg = JSON.parse(read("package.json"));
const current = pkg.version;
const m = /^(\d+)\.(\d+)\.(\d+)$/.exec(current);
if (!m) throw new Error(`package.json version "${current}" is not x.y.z`);
const [maj, min, pat] = m.slice(1).map(Number);
const arg = process.argv[2] ?? "patch";
let next;
if (arg === "patch") next = `${maj}.${min}.${pat + 1}`;
else if (arg === "minor") next = `${maj}.${min + 1}.0`;
else if (arg === "major") next = `${maj + 1}.0.0`;
else if (/^\d+\.\d+\.\d+$/.test(arg)) next = arg;
else throw new Error(`Use patch, minor, major or a version like 1.0.0, not "${arg}".`);

if (next === current) {
  console.log(`Already at ${current}. Nothing to change.`);
  process.exit(0);
}

// JSON files: change only the version fields, keep the formatting.
/** @param {string} p @param {number} count */
const swapJson = (p, count) => {
  const s = read(p);
  let n = 0;
  const out = s.replace(new RegExp(`("version":\\s*")${current.replace(/\./g, "\\.")}(")`, "g"), (all, a, b) => (n++ < count ? `${a}${next}${b}` : all));
  if (n === 0) throw new Error(`${p}: version ${current} not found`);
  write(p, out);
};
swapJson("package.json", 1);
swapJson("package-lock.json", 2); // the root entry and packages[""]
swapJson("src-tauri/tauri.conf.json", 1);

// Cargo.toml: the [package] version line.
const toml = read("src-tauri/Cargo.toml");
const toml2 = toml.replace(/(\[package\][\s\S]*?\nversion = ")[^"]+(")/, `$1${next}$2`);
if (toml2 === toml) throw new Error("src-tauri/Cargo.toml: package version not found");
write("src-tauri/Cargo.toml", toml2);

// Cargo.lock: the app's own entry.
const lock = read("src-tauri/Cargo.lock");
const lock2 = lock.replace(/(name = "audiform"\nversion = ")[^"]+(")/, `$1${next}$2`);
if (lock2 === lock) throw new Error("src-tauri/Cargo.lock: audiform entry not found");
write("src-tauri/Cargo.lock", lock2);

console.log(`Version ${current} -> ${next}. Next: commit, push, then tag v${next} once CI is green.`);
