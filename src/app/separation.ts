// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Audio Separation helpers with no app state, so they can be tested:
 * grouping the model list and building the tracks that hold results.
 */

import type { SeparationModel } from "./core";
import type { ClipDoc, TrackDoc } from "../stores/session";

/** Models by category, categories in the order they first appear, best score first within each (unscored last). */
export function byCategory(models: readonly SeparationModel[]): [string, SeparationModel[]][] {
  const groups = new Map<string, SeparationModel[]>();
  for (const m of models) groups.set(m.category, [...(groups.get(m.category) ?? []), m]);
  return [...groups].map(([c, list]) => [c, [...list].sort((a, b) => (b.sdr ?? -1) - (a.sdr ?? -1))]);
}

/**
 * What a model splits the sound into, as the list groups it: "Vocals and
 * instrumental", "Lead vocal and backing vocals and instruments", "Bass,
 * drums, other, vocals, guitar, piano". Models that make the same split
 * sit under one heading, best score first.
 */
export function splitOf(m: Pick<SeparationModel, "stems" | "targetName" | "restName"> & { group?: string }): string {
  if (m.group) return m.group;
  if (m.stems.length > 8) return `${m.stems.length} instruments at once`;
  return m.stems.length ? m.stems.map(([n]) => n).join(", ") : `${m.targetName} and ${m.restName}`;
}

/** Models of one tab grouped by what they split, each group best score first, groups in the order of their best model. */
export function bySplit(models: readonly SeparationModel[]): [string, SeparationModel[]][] {
  const groups = new Map<string, SeparationModel[]>();
  for (const m of models) {
    const k = splitOf(m);
    groups.set(k, [...(groups.get(k) ?? []), m]);
  }
  const sorted = [...groups].map(([k, list]): [string, SeparationModel[]] => [k, [...list].sort((a, b) => (b.sdr ?? -1) - (a.sdr ?? -1))]);
  return sorted;
}

/** Settings key of the user's memo on a model: what it works best for. */
export const MEMO_KEY = "separation.memos";

export function memosOf(raw: unknown): Record<string, string> {
  if (!raw || typeof raw !== "object") return {};
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(raw as Record<string, unknown>)) if (typeof v === "string" && v.trim()) out[k] = v;
  return out;
}

/** "SDR 11.00 dB", or "SDR not published". A several-stem model lists each stem's score. */
export function sdrText(m: Pick<SeparationModel, "sdr" | "stems">): string {
  // Stems with no published score (0) are left out.
  const scored = m.stems.filter(([, v]) => v > 0);
  if (scored.length > 0) return `SDR ${scored.map(([n, v]) => `${n.toLowerCase()} ${v.toFixed(1)}`).join(", ")} dB`;
  return m.sdr === null ? "SDR not published" : `SDR ${m.sdr.toFixed(2)} dB`;
}

/**
 * A copy of `track` holding `clips` (copies, playing the separated file
 * instead), with its effects, levels, routing, sends and automation.
 * Everything gets new ids, and automation of an effect follows that
 * effect's copy.
 */
export function separatedTrack(
  track: TrackDoc,
  clips: readonly ClipDoc[],
  media: (oldMediaId: string) => { id: string; length: number } | undefined,
  name: string,
  newId: () => string,
): TrackDoc {
  const slotIds = new Map<string, string>();
  const effects = track.effects
    ? {
        ...track.effects,
        slots: track.effects.slots.map((s) => {
          const id = newId();
          slotIds.set(s.id, id);
          return { ...s, id };
        }),
      }
    : track.effects;
  const envelopes = track.envelopes?.map((e) => ({
    ...e,
    id: newId(),
    target: e.target.type === "effect_param" ? { ...e.target, slot_id: slotIds.get(e.target.slot_id) ?? e.target.slot_id } : e.target,
  }));
  const copies: ClipDoc[] = [];
  for (const c of clips) {
    const m = media(c.media_id);
    if (!m) continue;
    copies.push({ ...c, id: newId(), media_id: m.id });
  }
  return {
    ...track,
    id: newId(),
    name,
    solo: false,
    record_armed: false,
    ...(effects ? { effects } : {}),
    ...(envelopes ? { envelopes } : {}),
    ...(track.sends ? { sends: track.sends.map((s) => ({ ...s, id: newId() })) } : {}),
    clips: copies,
  };
}
