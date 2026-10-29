// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { settings, type AppSettings } from "../stores/settings";

export const EXTRA_THEME_ID = "undercurrent";

const PHRASE_HASH = "65779cb97bc1a4fd0f152b7e470f1c1ed24a4b8eab9415dbcc6b51c9adbde071";

async function sha256Hex(text: string): Promise<string> {
  const bytes = new TextEncoder().encode(text);
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

export async function isPhrase(text: string): Promise<boolean> {
  const t = text.trim().toLowerCase();
  if (!/^[a-z]+$/.test(t) || !globalThis.crypto?.subtle) return false;
  return (await sha256Hex(t)) === PHRASE_HASH;
}

export async function tryUnlock(text: string): Promise<boolean> {
  if (!(await isPhrase(text))) return false;
  const s = settings.current;
  settings.set({ ...s, theme: EXTRA_THEME_ID, unlockedThemes: [...new Set([...s.unlockedThemes, EXTRA_THEME_ID])] });
  return true;
}

/** Settings with the unlock removed once another theme is in use, or null when nothing changes. */
export function relock(s: AppSettings): AppSettings | null {
  if (s.theme === EXTRA_THEME_ID || !s.unlockedThemes.includes(EXTRA_THEME_ID)) return null;
  return { ...s, unlockedThemes: s.unlockedThemes.filter((id) => id !== EXTRA_THEME_ID) };
}
