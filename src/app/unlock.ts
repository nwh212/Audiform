// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { settings, type AppSettings } from "../stores/settings";

export const EXTRA_THEME_ID = "undercurrent";

/** Typed phrases (as hashes) and the theme each unlocks. */
const UNLOCKS: { hash: string; id: string }[] = [
  { id: EXTRA_THEME_ID, hash: "65779cb97bc1a4fd0f152b7e470f1c1ed24a4b8eab9415dbcc6b51c9adbde071" },
  { id: "marigold", hash: "fa1eadc4c6995667412681c69ce33adfc9302a2965f521c40908549e670e2e4e" },
];

/** Every theme a phrase can unlock. */
export const EXTRA_THEME_IDS: readonly string[] = UNLOCKS.map((u) => u.id);

async function sha256Hex(text: string): Promise<string> {
  const bytes = new TextEncoder().encode(text);
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/** The theme a typed phrase unlocks, if it is one. Only letters are ever hashed, so ordinary times are not. */
export async function themeForPhrase(text: string): Promise<string | null> {
  const t = text.trim().toLowerCase();
  if (!/^[a-z]+$/.test(t) || !globalThis.crypto?.subtle) return null;
  const h = await sha256Hex(t);
  return UNLOCKS.find((u) => u.hash === h)?.id ?? null;
}

export async function isPhrase(text: string): Promise<boolean> {
  return (await themeForPhrase(text)) !== null;
}

export async function tryUnlock(text: string): Promise<boolean> {
  const id = await themeForPhrase(text);
  if (!id) return false;
  const s = settings.current;
  settings.set({ ...s, theme: id, unlockedThemes: [...new Set([...s.unlockedThemes, id])] });
  return true;
}

/** Settings with the unlocks removed for every theme not in use, or null when nothing changes. */
export function relock(s: AppSettings): AppSettings | null {
  const stale = s.unlockedThemes.filter((id) => EXTRA_THEME_IDS.includes(id) && id !== s.theme);
  if (stale.length === 0) return null;
  return { ...s, unlockedThemes: s.unlockedThemes.filter((id) => !stale.includes(id)) };
}
