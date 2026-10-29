// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * The hidden theme. Typing its phrase in Time Info's playhead box and
 * pressing Enter unlocks it and switches to it. Only a hash of the phrase
 * is kept here, so reading the source does not give it away. The theme is
 * forgotten as soon as another one is chosen: it has to be found again.
 */

import { settings, type AppSettings } from "../stores/settings";

export const SECRET_THEME_ID = "undercurrent";

const PHRASE_HASH = "65779cb97bc1a4fd0f152b7e470f1c1ed24a4b8eab9415dbcc6b51c9adbde071";

async function sha256Hex(text: string): Promise<string> {
  const bytes = new TextEncoder().encode(text);
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/** Whether typed text is the phrase. Only letters are ever hashed, so ordinary times are not. */
export async function isSecretPhrase(text: string): Promise<boolean> {
  const t = text.trim().toLowerCase();
  if (!/^[a-z]+$/.test(t) || !globalThis.crypto?.subtle) return false;
  return (await sha256Hex(t)) === PHRASE_HASH;
}

/** Unlocks and switches to the secret theme when the text is the phrase. */
export async function tryUnlockSecretTheme(text: string): Promise<boolean> {
  if (!(await isSecretPhrase(text))) return false;
  const s = settings.current;
  settings.set({ ...s, theme: SECRET_THEME_ID, unlockedThemes: [...new Set([...s.unlockedThemes, SECRET_THEME_ID])] });
  return true;
}

/** Settings with the secret theme locked again when another theme is in use, or null when nothing needs to change. */
export function forgetSecretTheme(s: AppSettings): AppSettings | null {
  if (s.theme === SECRET_THEME_ID || !s.unlockedThemes.includes(SECRET_THEME_ID)) return null;
  return { ...s, unlockedThemes: s.unlockedThemes.filter((id) => id !== SECRET_THEME_ID) };
}
