// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Key chords as text, for example "Ctrl+Shift+K". One canonical spelling
 * per chord: modifiers in the order Ctrl, Alt, Shift, then the key.
 * Letter keys are upper case. Named keys use fixed names (Space, Delete,
 * Backspace, Enter, ArrowLeft, F5).
 */

const MODIFIERS = ["Ctrl", "Alt", "Shift"] as const;

const NAMED: Record<string, string> = {
  " ": "Space",
  space: "Space",
  spacebar: "Space",
  del: "Delete",
  delete: "Delete",
  backspace: "Backspace",
  enter: "Enter",
  return: "Enter",
  esc: "Escape",
  escape: "Escape",
  tab: "Tab",
  insert: "Insert",
  ins: "Insert",
  home: "Home",
  end: "End",
  pageup: "PageUp",
  pagedown: "PageDown",
  up: "ArrowUp",
  arrowup: "ArrowUp",
  down: "ArrowDown",
  arrowdown: "ArrowDown",
  left: "ArrowLeft",
  arrowleft: "ArrowLeft",
  right: "ArrowRight",
  arrowright: "ArrowRight",
};

const MODIFIER_KEYS = new Set(["Control", "Alt", "Shift", "Meta", "AltGraph", "OS"]);

function canonicalKey(raw: string): string | null {
  if (raw.length === 1) return raw === " " ? "Space" : raw.toUpperCase();
  const lower = raw.toLowerCase();
  if (NAMED[lower]) return NAMED[lower];
  const f = /^f([1-9]|1[0-9]|2[0-4])$/.exec(lower);
  if (f) return `F${f[1]}`;
  return null;
}

/** Parses a chord string. Returns the canonical form, or null if invalid. */
export function parseChord(text: string): string | null {
  const trimmed = text.trim();
  if (!trimmed) return null;
  // A chord can end in "+" as its key, as in "Ctrl++".
  let keyPart: string;
  let modPart: string;
  if (trimmed.endsWith("++")) {
    keyPart = "+";
    modPart = trimmed.slice(0, -2);
  } else if (trimmed === "+") {
    keyPart = "+";
    modPart = "";
  } else {
    const i = trimmed.lastIndexOf("+");
    keyPart = i < 0 ? trimmed : trimmed.slice(i + 1);
    modPart = i < 0 ? "" : trimmed.slice(0, i);
  }

  const mods = new Set<string>();
  for (const m of modPart.split("+").filter(Boolean)) {
    const lower = m.trim().toLowerCase();
    if (lower === "ctrl" || lower === "control") mods.add("Ctrl");
    else if (lower === "alt") mods.add("Alt");
    else if (lower === "shift") mods.add("Shift");
    else return null;
  }

  const key = canonicalKey(keyPart.trim() === "" ? keyPart : keyPart.trim());
  if (!key) return null;
  return [...MODIFIERS.filter((m) => mods.has(m)), key].join("+");
}

export interface KeyEventLike {
  key: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
  /** The physical key (KeyT, Backspace), used when `key` does not name one. */
  code?: string;
}

/** The key a physical key code names, for the keys shortcuts use. */
function keyFromCode(code: string | undefined): string | null {
  if (!code) return null;
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return letter[1]!;
  const digit = /^(?:Digit|Numpad)([0-9])$/.exec(code);
  if (digit) return digit[1]!;
  return canonicalKey(code);
}

/** Builds the chord for a key event, or null for a lone modifier key. */
export function chordFromEvent(e: KeyEventLike): string | null {
  if (MODIFIER_KEYS.has(e.key)) return null;
  let key = canonicalKey(e.key);
  // On Windows, Ctrl+Alt counts as the AltGr key: the key then reads as
  // what AltGr types (a control character for Backspace, or a symbol on
  // some layouts), so the chord fell through. Read the physical key then.
  const c = e.key.length === 1 ? e.key.charCodeAt(0) : -1;
  const control = c >= 0 && (c < 0x20 || c === 0x7f);
  const altGr = e.ctrlKey && e.altKey && e.key.length === 1 && !/^[a-z0-9]$/i.test(e.key);
  if (!key || control || altGr) key = keyFromCode(e.code) ?? (control ? null : key);
  if (!key) return null;
  const mods: string[] = [];
  if (e.ctrlKey || e.metaKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  return [...mods, key].join("+");
}
