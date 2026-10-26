// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Track and bus automation lanes: which settings can move, each lane's
 * range, and the edits to an owner's envelopes. Positions are session
 * samples on the timeline. Values hold flat before the first point and
 * after the last and move in straight lines between points, as the core
 * plays them (`engine/automation.rs`).
 */

import type { EffectInfo, EffectParamInfo } from "../../app/core";
import { DEFAULT_SPACE_POS, type EffectChainDoc, type EnvelopeDoc, type EnvelopePointDoc, type SpaceParam, type SpacePosDoc } from "../../stores/session";

/** Height of one automation lane, in pixels. */
export const AUTO_H = 54;

export type AutoTarget = EnvelopeDoc["target"];

/** What a track or bus lane needs to know about its owner. */
export interface AutoOwner {
  volume_db: number;
  pan: number;
  effects?: EffectChainDoc;
  envelopes?: EnvelopeDoc[];
  /** 3D Space sessions: its spot in the room. */
  space?: SpacePosDoc | null;
  channels?: "mono" | "stereo";
  mono?: boolean;
}

/** The spot settings a lane can move, in the order the picker lists them. */
const SPACE_LANES: { param: SpaceParam; label: string; stereo?: boolean }[] = [
  { param: "x", label: "3D: Left / right" },
  { param: "y", label: "3D: Back / front" },
  { param: "z", label: "3D: Height" },
  { param: "size", label: "3D: Size" },
  { param: "spread", label: "3D: Spread" },
  { param: "width", label: "3D: Width", stereo: true },
  { param: "rotation", label: "3D: Rotation", stereo: true },
  { param: "spatialize", label: "3D: Spatialize", stereo: true },
  { param: "lfe", label: "3D: LFE send" },
];

export interface AutoRange {
  /** Value drawn at the top of the lane. */
  top: number;
  bottom: number;
  step: number;
  /** Drawn on a log scale (frequencies). */
  log: boolean;
  unit: string;
}

export const VOLUME_RANGE: AutoRange = { top: 12, bottom: -60, step: 0.1, log: false, unit: "dB" };
export const PAN_RANGE: AutoRange = { top: -100, bottom: 100, step: 1, log: false, unit: "" };

/** A stable key for a target, for pickers and lookups. */
export function targetKey(t: AutoTarget): string {
  if (t.type === "effect_param") return `fx:${t.slot_id}:${t.param}`;
  if (t.type === "space") return `space:${t.param}`;
  return t.type;
}

function paramOf(owner: AutoOwner, catalog: readonly EffectInfo[], t: AutoTarget): { info: EffectInfo; param: EffectParamInfo; value: number } | null {
  if (t.type !== "effect_param") return null;
  const slot = owner.effects?.slots.find((s) => s.id === t.slot_id);
  const info = slot ? catalog.find((e) => e.id === slot.effect) : undefined;
  const param = info?.params.find((p) => p.key === t.param && !p.hidden);
  if (!slot || !info || !param) return null;
  return { info, param, value: slot.params[param.key] ?? param.default };
}

/**
 * Every setting of an owner that a lane can move: volume, pan (or in a 3D
 * Space session, where pan does nothing, the spot's settings), then each
 * effect control in chain order.
 */
export function autoChoices(owner: AutoOwner, catalog: readonly EffectInfo[], in3d = false): { target: AutoTarget; label: string; group?: string; name?: string }[] {
  const out: { target: AutoTarget; label: string; group?: string; name?: string }[] = [{ target: { type: "volume" }, label: "Volume" }];
  if (in3d) {
    const stereo = owner.channels !== "mono" && !owner.mono;
    for (const l of SPACE_LANES) if (stereo || !l.stereo) out.push({ target: { type: "space", param: l.param }, label: l.label });
  } else out.push({ target: { type: "pan" }, label: "Pan" });
  for (const slot of owner.effects?.slots ?? []) {
    const info = catalog.find((e) => e.id === slot.effect);
    for (const p of info?.params ?? []) {
      // Look-ahead changes restart playback, and on/off or list controls do not glide.
      if (p.hidden || p.retimes || p.toggle || p.choices) continue;
      out.push({ target: { type: "effect_param", slot_id: slot.id, param: p.key }, label: `${info!.name}: ${p.name}`, group: `${info!.name}#${slot.id}`, name: p.name });
    }
  }
  return out;
}

/** The lane's name, for its header and undo. */
export function autoName(owner: AutoOwner, catalog: readonly EffectInfo[], t: AutoTarget): string {
  if (t.type === "volume") return "Volume";
  if (t.type === "pan") return "Pan";
  if (t.type === "space") return SPACE_LANES.find((l) => l.param === t.param)?.label ?? "3D";
  const p = paramOf(owner, catalog, t);
  return p ? `${p.info.name}: ${p.param.name}` : "Removed effect";
}

/** True when the lane's effect is no longer in the chain (the lane stays, and plays nothing). */
export function isOrphan(owner: AutoOwner, catalog: readonly EffectInfo[], t: AutoTarget): boolean {
  return t.type === "effect_param" && !paramOf(owner, catalog, t);
}

export function autoRange(owner: AutoOwner, catalog: readonly EffectInfo[], t: AutoTarget): AutoRange {
  if (t.type === "volume") return VOLUME_RANGE;
  if (t.type === "pan") return PAN_RANGE;
  if (t.type === "space") return spaceRange(t.param);
  const p = paramOf(owner, catalog, t);
  if (!p) return { top: 1, bottom: 0, step: 0.01, log: false, unit: "" };
  const { min, max, step, log, unit } = p.param;
  return { top: max, bottom: min, step: step > 0 ? step : 0.01, log: log && min > 0, unit };
}

/** Where a lane with no points sits: the fader, the pan, or the effect's own setting. */
export function autoRest(owner: AutoOwner, catalog: readonly EffectInfo[], t: AutoTarget): number {
  if (t.type === "volume") return owner.volume_db;
  if (t.type === "pan") return owner.pan;
  if (t.type === "space") return (owner.space ?? DEFAULT_SPACE_POS)[t.param] ?? DEFAULT_SPACE_POS[t.param] ?? 0;
  return paramOf(owner, catalog, t)?.value ?? 0;
}

/** 0 at the top of the lane to 1 at the bottom. */
function share(v: number, r: AutoRange): number {
  const lo = Math.min(r.top, r.bottom);
  const hi = Math.max(r.top, r.bottom);
  const c = Math.max(lo, Math.min(hi, v));
  if (r.log) return Math.log(c / r.top) / Math.log(r.bottom / r.top);
  return (c - r.top) / (r.bottom - r.top || 1);
}

export function autoY(v: number, r: AutoRange, top: number, height: number): number {
  return top + share(v, r) * height;
}

/** A y position to a value, rounded to the lane's step. */
export function autoValue(y: number, r: AutoRange, top: number, height: number): number {
  const f = Math.max(0, Math.min(1, (y - top) / Math.max(1, height)));
  const v = r.log ? r.top * Math.pow(r.bottom / r.top, f) : r.top + f * (r.bottom - r.top);
  const s = r.log ? Math.min(r.step, Math.abs(v) / 100 || r.step) : r.step;
  const lo = Math.min(r.top, r.bottom);
  const hi = Math.max(r.top, r.bottom);
  return Math.max(lo, Math.min(hi, Math.round(v / s) * s));
}

/** Pulls a dragged value onto 0 dB or center when close (hold Alt to place it freely). */
export function autoSnap(t: AutoTarget, v: number): number {
  if (t.type === "volume" && Math.abs(v) <= 0.75) return 0;
  if (t.type === "pan" && Math.abs(v) <= 4) return 0;
  return v;
}

/** A spot setting's range: left at the top for left/right (as pan), front, high or more at the top otherwise. */
function spaceRange(p: SpaceParam): AutoRange {
  if (p === "x") return { top: -1, bottom: 1, step: 0.01, log: false, unit: "" };
  if (p === "y" || p === "z") return { top: 1, bottom: -1, step: 0.01, log: false, unit: "" };
  if (p === "rotation") return { top: 180, bottom: -180, step: 1, log: false, unit: "°" };
  return { top: 1, bottom: 0, step: 0.01, log: false, unit: "%" };
}

export function autoFormat(t: AutoTarget, v: number, r: AutoRange): string {
  if (t.type === "space") {
    if (t.param === "x") return Math.abs(v) < 0.005 ? "Center" : v < 0 ? `Left ${Math.round(-v * 100)}%` : `Right ${Math.round(v * 100)}%`;
    if (t.param === "y") return Math.abs(v) < 0.005 ? "Middle" : v > 0 ? `Front ${Math.round(v * 100)}%` : `Back ${Math.round(-v * 100)}%`;
    if (t.param === "rotation") return `${Math.round(v)}°`;
    return `${Math.round(v * 100)}%`;
  }
  if (t.type === "volume") return v <= -59.9 ? "Silent" : `${v > 0 ? "+" : ""}${v.toFixed(1)} dB`;
  if (t.type === "pan") {
    const p = Math.round(v);
    return p === 0 ? "C" : p < 0 ? `L ${-p}` : `R ${p}`;
  }
  const n = Math.abs(v) >= 1000 ? v.toFixed(0) : Math.abs(v) >= 100 ? v.toFixed(0) : Math.abs(v) >= 10 ? v.toFixed(1) : v.toFixed(2);
  return r.unit ? `${n} ${r.unit}` : n;
}

/** The value at timeline sample `pos`, or `rest` with no points. */
export function autoAt(points: readonly EnvelopePointDoc[], pos: number, rest: number): number {
  if (points.length === 0) return rest;
  const first = points[0]!;
  const last = points[points.length - 1]!;
  if (pos <= first.position) return first.value;
  if (pos >= last.position) return last.value;
  for (let i = 1; i < points.length; i++) {
    const b = points[i]!;
    if (pos <= b.position) {
      const a = points[i - 1]!;
      const f = (pos - a.position) / Math.max(1, b.position - a.position);
      return a.value + (b.value - a.value) * f;
    }
  }
  return last.value;
}

const newId = (): string =>
  typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : "00000000-0000-4000-8000-".concat(Math.floor(Math.random() * 2 ** 48).toString(16).padStart(12, "0"));

/** A new, empty lane for `target`. */
export function newLane(target: AutoTarget): EnvelopeDoc {
  return { id: newId(), target, enabled: true, points: [] };
}

/**
 * The lanes an owner shows: its envelopes in order, or a Volume lane to
 * start from when it has none (saved only once a point is added).
 */
export function shownLanes(owner: AutoOwner): EnvelopeDoc[] {
  const envs = owner.envelopes ?? [];
  return envs.length > 0 ? envs : [{ id: "", target: { type: "volume" }, enabled: true, points: [] }];
}

/** The owner's envelopes with `lane` put in (replacing the one with its id, or added at the end). */
export function putLane(owner: AutoOwner, lane: EnvelopeDoc): EnvelopeDoc[] {
  const envs = owner.envelopes ?? [];
  const l = lane.id ? lane : { ...lane, id: newId() };
  const i = envs.findIndex((e) => e.id === l.id);
  if (i < 0) return [...envs, l];
  return envs.map((e, k) => (k === i ? l : e));
}

export function dropLane(owner: AutoOwner, id: string): EnvelopeDoc[] {
  return (owner.envelopes ?? []).filter((e) => e.id !== id);
}

/** The setting a new lane takes: the first one that has no lane yet. */
export function nextTarget(owner: AutoOwner, catalog: readonly EffectInfo[], in3d = false): AutoTarget | null {
  const used = new Set(shownLanes(owner).map((e) => targetKey(e.target)));
  return autoChoices(owner, catalog, in3d).find((c) => !used.has(targetKey(c.target)))?.target ?? null;
}

/** Adds a point at `pos`, or moves the one already there. Returns the list and the point's index. */
export function autoAdd(points: readonly EnvelopePointDoc[], pos: number, value: number): { points: EnvelopePointDoc[]; index: number } {
  const p = Math.max(0, Math.round(pos));
  const next = points.filter((x) => x.position !== p).concat({ position: p, value, curve: "linear" });
  next.sort((a, b) => a.position - b.position);
  return { points: next, index: next.findIndex((x) => x.position === p) };
}

/** Moves point `i` to `pos` and `value`, kept between its neighbors. */
export function autoMove(points: readonly EnvelopePointDoc[], i: number, pos: number, value: number): EnvelopePointDoc[] {
  const lo = i > 0 ? points[i - 1]!.position + 1 : 0;
  const hi = i < points.length - 1 ? points[i + 1]!.position - 1 : Number.MAX_SAFE_INTEGER;
  return points.map((p, k) => (k === i ? { ...p, position: Math.round(Math.max(lo, Math.min(hi, pos))), value } : p));
}

/** The track's automation mode as its header shows it: Read while any lane plays, Off when every lane is off. */
export function autoMode(owner: AutoOwner): "read" | "off" {
  const envs = owner.envelopes ?? [];
  return envs.length === 0 || envs.some((e) => e.enabled) ? "read" : "off";
}

/** Every lane on (Read) or off. */
export function setMode(owner: AutoOwner, mode: "read" | "off"): EnvelopeDoc[] {
  return (owner.envelopes ?? []).map((e) => ({ ...e, enabled: mode === "read" }));
}
