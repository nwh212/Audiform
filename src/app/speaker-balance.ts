// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Multitrack > Speaker Balance: brings every voice track to the same
 * loudness in one click, before the FX Chain, so gates and
 * compressors see each speaker at a like level. The quietest voice is
 * the reference and the others come down to it, which never pushes
 * anyone into clipping. Measured raw (no FX Chain, no faders);
 * applied as clip gain, one undoable edit. Music, sound effect and
 * ambience tracks are left alone.
 */

import type { LoudnessReport } from "./loudness";
import { matchChanges, type MatchItem, type MatchRow } from "./loudness-match";
import type { ClipChange, SessionDoc, TrackDoc } from "../stores/session";

/** Differences smaller than this are left alone, in LU. */
export const BALANCE_TOLERANCE = 0.5;

/** A voice track: has clips, is not muted, and is not all music, effects or ambience. */
export function isVoiceTrack(t: TrackDoc): boolean {
  if (t.muted || t.clips.length === 0) return false;
  return t.clips.some((c) => !c.audio_type || c.audio_type === "dialogue");
}

/** What to measure: each voice track, all its voice clips together. */
export function balanceItems(doc: SessionDoc): MatchItem[] {
  return doc.tracks.filter(isVoiceTrack).map((t) => ({
    key: t.id,
    label: t.name,
    clipIds: t.clips.filter((c) => !c.audio_type || c.audio_type === "dialogue").map((c) => c.id),
  }));
}

export interface Balance {
  rows: MatchRow[];
  /** The quietest measurable track, which stays where it is. */
  reference: MatchRow | null;
  changes: ClipChange[];
}

/** Rows that bring each measured track to the quietest one. */
export function balance(doc: SessionDoc, items: readonly MatchItem[], reports: readonly LoudnessReport[]): Balance {
  const rows: MatchRow[] = items.map((item, i) => {
    const r = reports[i];
    const measured = r ? r.integrated : Number.NEGATIVE_INFINITY;
    const ok = r !== undefined && Number.isFinite(measured) && measured > -70;
    return { ...item, measured, truePeak: r?.truePeak ?? Number.NEGATIVE_INFINITY, change: 0, peakAfter: r?.truePeak ?? 0, measurable: ok, peakWarning: false };
  });
  const measured = rows.filter((r) => r.measurable);
  if (measured.length < 2) return { rows, reference: measured[0] ?? null, changes: [] };
  const reference = measured.reduce((a, b) => (b.measured < a.measured ? b : a));
  for (const r of measured) {
    const diff = reference.measured - r.measured;
    r.change = Math.abs(diff) < BALANCE_TOLERANCE ? 0 : Math.round(diff * 10) / 10;
    r.peakAfter = Math.round((r.truePeak + r.change) * 10) / 10;
  }
  return { rows, reference, changes: matchChanges(doc, rows) };
}

/** One line for the notice. */
export function describeBalance(b: Balance): string {
  if (!b.reference) return "No voice tracks with audio to balance.";
  const moved = b.rows.filter((r) => r.measurable && r.change !== 0);
  if (b.rows.filter((r) => r.measurable).length < 2) return "Speaker Balance needs at least two voice tracks with audio.";
  if (moved.length === 0) return `The voices already sit within ${BALANCE_TOLERANCE} LU of each other.`;
  const list = moved.map((r) => `${r.label} ${r.change > 0 ? "+" : ""}${r.change.toFixed(1)} dB`).join(", ");
  return `Balanced to ${b.reference.label} (${b.reference.measured.toFixed(1)} LUFS): ${list}.`;
}
