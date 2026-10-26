// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Fader travel to dB and back. The top 70% of the travel covers -24 dB
 * to +12 dB, where voice levels live; the bottom 30% covers -24 dB down
 * to silence. That gives fine control where it matters.
 */

export const FADER_MAX_DB = 12;
export const FADER_MIN_DB = -60;
const KNEE_DB = -24;
const KNEE_POS = 0.3;

/** Fader position (0 bottom, 1 top) for a level in dB. */
export function dbToFader(db: number): number {
  if (db <= FADER_MIN_DB) return 0;
  if (db >= FADER_MAX_DB) return 1;
  if (db >= KNEE_DB) return KNEE_POS + ((db - KNEE_DB) / (FADER_MAX_DB - KNEE_DB)) * (1 - KNEE_POS);
  return ((db - FADER_MIN_DB) / (KNEE_DB - FADER_MIN_DB)) * KNEE_POS;
}

/** Level in dB for a fader position, rounded to 0.1 dB. The bottom means silence (-96 dB). */
export function faderToDb(pos: number): number {
  const p = Math.min(1, Math.max(0, pos));
  if (p <= 0.001) return -96;
  const db =
    p >= KNEE_POS
      ? KNEE_DB + ((p - KNEE_POS) / (1 - KNEE_POS)) * (FADER_MAX_DB - KNEE_DB)
      : FADER_MIN_DB + (p / KNEE_POS) * (KNEE_DB - FADER_MIN_DB);
  return Math.round(db * 10) / 10;
}

export function formatDb(db: number): string {
  if (db <= -95.9) return "-inf";
  return db > 0 ? `+${db.toFixed(1)}` : db.toFixed(1);
}

export function formatPan(p: number): string {
  const r = Math.round(p);
  return r === 0 ? "C" : r < 0 ? `L${-r}` : `R${r}`;
}
