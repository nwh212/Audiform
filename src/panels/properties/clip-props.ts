// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

/**
 * The Properties panel's stretch and pitch settings as clip changes.
 * The panel shows one set of settings; a change applies to every
 * selected clip.
 */

import { naturalLength, stretchTo, unstretch } from "../editor/stretch";
import { PITCH_MAX, PITCH_MIN, STRETCH_MAX, STRETCH_MIN, type ClipChange, type ClipDoc, type MediaRefDoc, type StretchAdvancedDoc, type StretchDoc } from "../../stores/session";

/** Off: plays at its natural speed and pitch. Live and Rendered sound the same here. */
export type StretchMode = "off" | "live" | "rendered";
/** Single voice, several notes at once, or pitch with speed like tape. */
export type StretchType = "mono" | "poly" | "varispeed";

export interface StretchSettings {
  mode: StretchMode;
  type: StretchType;
  /** Percent of the natural length, 25 to 400. */
  percent: number;
  /** Semitones, -24 to +24. Ignored for varispeed. */
  pitch: number;
  advanced: StretchAdvancedDoc;
}

export const DEFAULT_ADVANCED: StretchAdvancedDoc = {
  window_ms: 30,
  transient_sensitivity: 40,
  preserve_formants: true,
  high_precision: true,
};

/** The settings a clip has now. */
export function settingsOf(clip: ClipDoc): StretchSettings {
  const s = clip.looped ? null : clip.stretch;
  if (!s) return { mode: "off", type: "mono", percent: 100, pitch: 0, advanced: { ...DEFAULT_ADVANCED } };
  return {
    mode: s.rendered ? "rendered" : "live",
    type: !s.keep_pitch ? "varispeed" : s.polyphonic ? "poly" : "mono",
    percent: Math.round(s.ratio * 10_000) / 100,
    pitch: s.keep_pitch ? (s.pitch ?? 0) : 0,
    advanced: { ...DEFAULT_ADVANCED, ...(s.advanced ?? {}) },
  };
}

/** Pitch change of varispeed at a ratio, in semitones (slower is lower). */
export function varispeedSemitones(ratio: number): number {
  return -12 * Math.log2(ratio);
}

const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));

/**
 * The change that gives `clip` these settings, or null when nothing
 * changes. Length changes also scale the clip's fades and envelopes.
 */
export function changeFor(clip: ClipDoc, m: MediaRefDoc | undefined, rate: number, next: StretchSettings): ClipChange | null {
  if (clip.looped) return null;
  if (next.mode === "off") return clip.stretch ? unstretch(clip, m, rate) : null;
  const ratio = clamp(next.percent / 100, STRETCH_MIN, STRETCH_MAX);
  const keepPitch = next.type !== "varispeed";
  const stretch: StretchDoc = {
    ratio,
    keep_pitch: keepPitch,
    pitch: keepPitch ? clamp(Math.round(next.pitch * 100) / 100, PITCH_MIN, PITCH_MAX) : 0,
    polyphonic: next.type === "poly",
    rendered: next.mode === "rendered",
    advanced: { ...next.advanced },
  };
  if (same(clip.stretch, stretch)) return null;
  const natural = naturalLength(clip, m, rate);
  const fit = stretchTo(clip, m, rate, Math.round(natural * ratio), keepPitch);
  const change: ClipChange = fit ? { ...fit.change } : { id: clip.id };
  delete change.unstretch;
  change.stretch = stretch;
  return change;
}

function same(a: StretchDoc | null | undefined, b: StretchDoc): boolean {
  if (!a) return false;
  const adv = (x: StretchDoc) => JSON.stringify(x.advanced ?? null);
  return (
    Math.abs(a.ratio - b.ratio) < 1e-9 &&
    a.keep_pitch === b.keep_pitch &&
    (a.pitch ?? 0) === (b.pitch ?? 0) &&
    !!a.polyphonic === !!b.polyphonic &&
    !!a.rendered === !!b.rendered &&
    adv(a) === adv(b)
  );
}
