// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Clip volume and pan envelopes: reading points, mapping values to the lane, and
 * keeping points lined up with the audio when a clip is trimmed or
 * split. The level is held flat before the first point and after the
 * last, and moves in straight lines (in dB) between points, the same as
 * playback.
 */

import type { ClipChange, ClipDoc, EnvelopePointDoc } from "../../stores/session";
import type { EffectInfo, EffectParamInfo } from "../../app/core";

export const DB_MIN = -60;
export const DB_MAX = 12;

/** Which clip envelope. "fx" is one setting of an effect in the clip's Voice Chain. */
export type EnvKind = "volume" | "pan" | "fx";

export interface EnvRange {
  top: number;
  bottom: number;
  step: number;
}

/**
 * Range of each envelope. `top` is the value drawn at the top of the
 * lane: +12 dB for volume, full left for pan.
 */
export const RANGE: Record<"volume" | "pan", EnvRange> = {
  volume: { top: DB_MAX, bottom: DB_MIN, step: 0.1 },
  pan: { top: -100, bottom: 100, step: 1 },
};

// --- Effect settings over time ---

/** One effect setting on a clip that can have an envelope. */
export interface FxLane {
  slot: string;
  param: string;
}

let fxCatalog = new Map<string, EffectInfo>();

/** The effect catalog, so effect envelopes know each control's range. */
export function setFxCatalog(list: readonly EffectInfo[]): void {
  fxCatalog = new Map(list.map((e) => [e.id, e]));
}

/** The control behind `lane` on `clip`, if that effect is still in its chain. */
export function fxParam(clip: ClipDoc, lane: FxLane | null): { info: EffectInfo; param: EffectParamInfo; value: number } | null {
  if (!lane) return null;
  const slot = clip.effects?.slots.find((s) => s.id === lane.slot);
  const info = slot ? fxCatalog.get(slot.effect) : undefined;
  const param = info?.params.find((p) => p.key === lane.param && !p.hidden);
  if (!slot || !info || !param) return null;
  return { info, param, value: slot.params[param.key] ?? param.default };
}

/** Effect settings of a clip's chain that can move over time. */
export function fxChoices(clip: ClipDoc): { lane: FxLane; label: string }[] {
  const out: { lane: FxLane; label: string }[] = [];
  for (const slot of clip.effects?.slots ?? []) {
    const info = fxCatalog.get(slot.effect);
    for (const p of info?.params ?? []) {
      if (p.hidden || p.retimes) continue;
      out.push({ lane: { slot: slot.id, param: p.key }, label: `${info!.name}: ${p.name}` });
    }
  }
  return out;
}

/**
 * The effect envelope shown on `clip`: the one picked for it, else the
 * first one it has.
 */
export function fxLaneOf(clip: ClipDoc, picked: Readonly<Record<string, FxLane>>): FxLane | null {
  const p = picked[clip.id];
  if (p && fxParam(clip, p)) return p;
  const e = clip.envelopes?.find((x) => x.target.type === "effect_param" && x.enabled);
  return e && e.target.type === "effect_param" ? { slot: e.target.slot_id, param: e.target.param } : null;
}

/** Range of an envelope; effect envelopes take the control's range (high at the top). */
export function rangeOf(kind: EnvKind, clip?: ClipDoc, lane?: FxLane | null): EnvRange {
  if (kind !== "fx") return RANGE[kind];
  const fp = clip ? fxParam(clip, lane ?? null) : null;
  if (!fp) return { top: 1, bottom: 0, step: 0.01 };
  return { top: fp.param.max, bottom: fp.param.min, step: fp.param.step > 0 ? fp.param.step : 0.01 };
}

export function pointsOf(clip: ClipDoc, kind: EnvKind, lane?: FxLane | null): EnvelopePointDoc[] {
  if (kind === "fx") {
    if (!lane) return [];
    return (
      clip.envelopes?.find((e) => e.target.type === "effect_param" && e.target.slot_id === lane.slot && e.target.param === lane.param && e.enabled)?.points ?? []
    );
  }
  return clip.envelopes?.find((e) => e.target.type === kind && e.enabled)?.points ?? [];
}

/** The value a line with no points sits at: 0 dB, centered, or the effect's own setting. */
export function restValue(kind: EnvKind, clip?: ClipDoc, lane?: FxLane | null): number {
  if (kind !== "fx" || !clip) return 0;
  return fxParam(clip, lane ?? null)?.value ?? 0;
}

/** The clip change that sets an envelope's points. */
export function envChange(clip: ClipDoc, kind: EnvKind, points: EnvelopePointDoc[], lane?: FxLane | null): ClipChange {
  if (kind === "volume") return { id: clip.id, volume: points };
  if (kind === "pan") return { id: clip.id, pan: points };
  return { id: clip.id, effect_points: [{ slot_id: lane!.slot, param: lane!.param, points }] };
}

/** Name of an envelope for menus and undo. */
export function envName(kind: EnvKind, clip?: ClipDoc, lane?: FxLane | null): string {
  if (kind === "volume") return "Volume";
  if (kind === "pan") return "Pan";
  const fp = clip ? fxParam(clip, lane ?? null) : null;
  return fp ? `${fp.info.name} ${fp.param.name}` : "Effect";
}

export function volumePoints(clip: ClipDoc): EnvelopePointDoc[] {
  return pointsOf(clip, "volume");
}

/** Value to a y position in a box `top` to `top + height`. */
export function valueToY(kind: EnvKind, v: number, top: number, height: number, range: EnvRange = rangeOf(kind)): number {
  const r = range;
  const lo = Math.min(r.top, r.bottom);
  const hi = Math.max(r.top, r.bottom);
  const c = Math.max(lo, Math.min(hi, v));
  return top + ((c - r.top) / (r.bottom - r.top)) * height;
}

/** A y position to a value, rounded to the envelope's step. */
export function yToValue(kind: EnvKind, y: number, top: number, height: number, range: EnvRange = rangeOf(kind)): number {
  const r = range;
  const f = Math.max(0, Math.min(1, (y - top) / Math.max(1, height)));
  const v = r.top + f * (r.bottom - r.top);
  return Math.round(v / r.step) * r.step;
}

/** Volume points snap to 0 dB within this, pan points to center within this. */
export const SNAP_DB = 0.75;
export const SNAP_PAN = 4;

/** A dragged value pulled onto 0 dB or center when close (hold Alt to place it freely). */
export function snapValue(kind: EnvKind, v: number): number {
  if (kind === "volume" && Math.abs(v) <= SNAP_DB) return 0;
  if (kind === "pan" && Math.abs(v) <= SNAP_PAN) return 0;
  return v;
}

/** Clamps a value to the envelope's range. */
export function clampValue(kind: EnvKind, v: number, range: EnvRange = rangeOf(kind)): number {
  const r = range;
  return Math.max(Math.min(r.top, r.bottom), Math.min(Math.max(r.top, r.bottom), v));
}

/** "L 40", "C", "R 25" for pan; "-6.0 dB" for volume; the value and unit for an effect. */
export function formatValue(kind: EnvKind, v: number, unit = ""): string {
  if (kind === "fx") {
    const t = Math.abs(v) >= 100 ? v.toFixed(0) : Math.abs(v) >= 10 ? v.toFixed(1) : v.toFixed(2);
    return unit ? `${t} ${unit}` : t;
  }
  if (kind === "volume") return `${v > 0 ? "+" : ""}${v.toFixed(1)} dB`;
  const p = Math.round(v);
  return p === 0 ? "C" : p < 0 ? `L ${-p}` : `R ${p}`;
}

/** Level in dB at `pos` (samples from the clip start). 0 dB when there are no points. */
export function levelAt(points: readonly EnvelopePointDoc[], pos: number): number {
  if (points.length === 0) return 0;
  const first = points[0]!;
  const last = points[points.length - 1]!;
  if (pos <= first.position) return first.value;
  if (pos >= last.position) return last.value;
  for (let i = 1; i < points.length; i++) {
    const b = points[i]!;
    if (pos <= b.position) {
      const a = points[i - 1]!;
      const t = (pos - a.position) / Math.max(1, b.position - a.position);
      return a.value + (b.value - a.value) * t;
    }
  }
  return last.value;
}

/** dB to a y position in a box `top` to `top + height`: +12 at the top, -60 at the bottom. */
export function dbToY(db: number, top: number, height: number): number {
  const d = Math.max(DB_MIN, Math.min(DB_MAX, db));
  return top + ((DB_MAX - d) / (DB_MAX - DB_MIN)) * height;
}

export function yToDb(y: number, top: number, height: number): number {
  const f = Math.max(0, Math.min(1, (y - top) / Math.max(1, height)));
  return Math.round((DB_MAX - f * (DB_MAX - DB_MIN)) * 10) / 10;
}

/**
 * The points for the part of a clip from `from` to `to` (samples from
 * its start), moved so `from` becomes 0. Levels at both ends are kept
 * by adding a point there, so the cut piece sounds the same.
 */
export function sliceVolume(points: readonly EnvelopePointDoc[], from: number, to: number): EnvelopePointDoc[] {
  if (points.length === 0) return [];
  const inside = points.filter((p) => p.position > from && p.position < to).map((p) => ({ ...p, position: p.position - from }));
  const curve = points[0]!.curve;
  return [
    { position: 0, value: levelAt(points, from), curve },
    ...inside,
    { position: Math.max(1, to - from), value: levelAt(points, to), curve },
  ];
}

/** Adds a point at `pos`, or moves the one already there. Returns the new list and the point's index. */
export function addPoint(points: readonly EnvelopePointDoc[], pos: number, value: number): { points: EnvelopePointDoc[]; index: number } {
  const p = Math.max(0, Math.round(pos));
  const next = points.filter((x) => x.position !== p).concat({ position: p, value, curve: "linear" });
  next.sort((a, b) => a.position - b.position);
  return { points: next, index: next.findIndex((x) => x.position === p) };
}

/** Moves point `i` to `pos` and `value`, keeping it between its neighbors. */
export function movePoint(
  points: readonly EnvelopePointDoc[],
  i: number,
  pos: number,
  value: number,
  len: number,
  kind: EnvKind = "volume",
  range: EnvRange = rangeOf(kind),
): EnvelopePointDoc[] {
  const lo = i > 0 ? points[i - 1]!.position + 1 : 0;
  const hi = i < points.length - 1 ? points[i + 1]!.position - 1 : len;
  const next = points.map((p) => ({ ...p }));
  next[i] = { ...next[i]!, position: Math.round(Math.max(lo, Math.min(hi, pos))), value: clampValue(kind, value, range) };
  return next;
}

/** Label of the envelope chip in a clip's title bar. */
export function chipLabel(showVolume: boolean, showPan: boolean, showFx = false): string {
  const on = [showVolume && "Vol", showPan && "Pan", showFx && "FX"].filter(Boolean) as string[];
  if (on.length > 1) return on.join(" + ");
  if (showVolume) return "Volume";
  if (showPan) return "Pan";
  if (showFx) return "Effect";
  return "Envelopes";
}

/**
 * Where the envelope chip sits in a clip's title bar: right-aligned,
 * clear of the fade-out handle. Null when the clip is too narrow.
 * Width comes from the label length so drawing and hit-testing agree.
 */
export function chipBox(x0: number, x1: number, label: string): { x: number; w: number } | null {
  const w = Math.round(label.length * 5.6 + 22);
  if (x1 - x0 < w + 70) return null;
  return { x: x1 - w - 14, w };
}
