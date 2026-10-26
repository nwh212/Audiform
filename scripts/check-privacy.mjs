// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Privacy guard. Fails the build if code that could reach the network,
 * track people or ask for accounts creeps in. See PRIVACY.md.
 *
 * Checks the interface sources, the core sources, the Rust dependency
 * list and the Tauri config. A real need can be discussed in a pull
 * request; it is never added quietly.
 */

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const root = new URL("..", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
/** @type {string[]} */
const problems = [];

/**
 * @param {string} dir
 * @param {string[]} exts
 * @param {string[]} [out]
 * @returns {string[]}
 */
function walk(dir, exts, out = []) {
  for (const name of readdirSync(dir)) {
    if (name === "node_modules" || name === "target" || name.startsWith(".")) continue;
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, exts, out);
    else if (exts.some((e) => name.endsWith(e)) && !/\.test\.ts$/.test(name)) out.push(p);
  }
  return out;
}

// Interface: no network APIs and no tracking or analytics libraries.
/** @type {[RegExp, string][]} */
const webBanned = [
  [/\bfetch\s*\(/, "fetch()"],
  [/\bXMLHttpRequest\b/, "XMLHttpRequest"],
  [/\bnew\s+WebSocket\b/, "WebSocket"],
  [/\bsendBeacon\s*\(/, "navigator.sendBeacon"],
  [/\bnew\s+EventSource\b/, "EventSource"],
  [/from\s+["'][^"']*(sentry|posthog|mixpanel|amplitude|segment|analytics|telemetry)[^"']*["']/i, "a tracking or analytics library"],
  [/\bgtag\s*\(/, "Google Analytics"],
];
for (const f of walk(join(root, "src"), [".ts", ".svelte"])) {
  const text = readFileSync(f, "utf8");
  for (const [re, what] of webBanned) {
    if (re.test(text)) problems.push(`${relative(root, f)}: uses ${what}`);
  }
}

// Core: no HTTP client crates, no telemetry crates.
const crateBanned = /^\s*(reqwest|ureq|hyper|isahc|surf|attohttpc|curl|tauri-plugin-http|tauri-plugin-updater|sentry[\w-]*|opentelemetry[\w-]*)\s*=/m;
const cargo = readFileSync(join(root, "src-tauri", "Cargo.toml"), "utf8");
const m = cargo.match(crateBanned);
if (m) problems.push(`src-tauri/Cargo.toml: depends on ${m[1]}`);
for (const f of walk(join(root, "src-tauri", "src"), [".rs"])) {
  const text = readFileSync(f, "utf8");
  if (/std::net::|TcpStream|UdpSocket/.test(text)) problems.push(`${relative(root, f)}: opens network sockets`);
}

// Tauri config: the page may only load the app's own files.
const conf = JSON.parse(readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8"));
const csp = conf?.app?.security?.csp ?? "";
if (!/default-src 'self'/.test(csp) || /connect-src|https?:/.test(csp)) {
  problems.push("src-tauri/tauri.conf.json: the content security policy must stay default-src 'self' with no remote hosts");
}

if (problems.length) {
  console.error("Privacy check failed. See PRIVACY.md.\n" + problems.map((p) => `  - ${p}`).join("\n"));
  process.exit(1);
}
console.log("Privacy check OK: no network, tracking or account code.");
