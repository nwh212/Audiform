// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Loudness Match: brings each selected clip, or each track, to a
 * loudness target by changing clip gain. The measuring happens in the
 * core; this file turns measurements into rows and rows into one edit.
 */

import type { LoudnessReport, LoudnessTarget } from "./loudness";
import type { ClipChange, SessionDoc } from "../stores/session";

/** Clip gain limits, matching the Clip Gain command. */
export const GAIN_MIN = -96;
export const GAIN_MAX = 24;

export interface MatchItem {
  key: string;
  label: string;
  /** The clips whose gain changes together. */
  clipIds: string[];
}

export interface MatchRow extends MatchItem {
  /** Integrated loudness as the target measures it (stereo or mono). */
  measured: number;
  truePeak: number;
  /** Gain change in dB, rounded to 0.1. */
  change: number;
  peakAfter: number;
  /** False when the item is silent or too short to measure. */
  measurable: boolean;
  /** True when the change would push peaks over the target's limit. */
  peakWarning: boolean;
}

export function matchRow(item: MatchItem, r: LoudnessReport, t: LoudnessTarget): MatchRow {
  const measured = t.mono ? r.integratedMono : r.integrated;
  if (!Number.isFinite(measured)) {
    return { ...item, measured, truePeak: r.truePeak, change: 0, peakAfter: r.truePeak, measurable: false, peakWarning: false };
  }
  const change = Math.round((t.lufs - measured) * 10) / 10;
  const peakAfter = Math.round((r.truePeak + change) * 10) / 10;
  return { ...item, measured, truePeak: r.truePeak, change, peakAfter, measurable: true, peakWarning: peakAfter > t.peak };
}

/** The clip gain changes for the chosen rows, as one edit. Gain stays within the clip gain limits. */
export function matchChanges(doc: SessionDoc, rows: readonly MatchRow[]): ClipChange[] {
  const gain = new Map<string, number>();
  for (const t of doc.tracks) for (const c of t.clips) gain.set(c.id, c.gain_db);
  const out: ClipChange[] = [];
  for (const row of rows) {
    if (!row.measurable || row.change === 0) continue;
    for (const id of row.clipIds) {
      const cur = gain.get(id);
      if (cur === undefined) continue;
      const next = Math.round(Math.max(GAIN_MIN, Math.min(GAIN_MAX, cur + row.change)) * 10) / 10;
      if (next !== cur) out.push({ id, gain_db: next });
    }
  }
  return out;
}

/** What to measure: the selected clips one by one, or else each track that has clips. */
export function matchItems(doc: SessionDoc, selected: ReadonlySet<string>): { mode: "clips" | "tracks"; items: MatchItem[] } {
  if (selected.size > 0) {
    const items: MatchItem[] = [];
    for (const t of doc.tracks) for (const c of t.clips) if (selected.has(c.id)) items.push({ key: c.id, label: `${c.name} (${t.name})`, clipIds: [c.id] });
    return { mode: "clips", items };
  }
  return {
    mode: "tracks",
    items: doc.tracks.filter((t) => t.clips.length > 0 && !t.muted).map((t) => ({ key: t.id, label: t.name, clipIds: t.clips.map((c) => c.id) })),
  };
}
