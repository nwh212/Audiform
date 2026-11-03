// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Automation writing: while the session plays, moves of a track's or
 * bus's controls (its fader, pan, spot in the room, or any effect
 * setting) become points on the matching automation lane, by the owner's
 * automation mode:
 *
 * - Read: nothing is written; the lanes play as they are.
 * - Touch: a lane is written while its control is held (moved within the
 *   last moment) and plays again once it is let go.
 * - Latch: a lane is written from the first move of its control and keeps
 *   the last value until play stops.
 * - Write: every lane the owner has is written from the moment play
 *   starts, with its control's value, over whatever was there; a move
 *   changes what is written from then on.
 *
 * Every edit goes through `SessionStore.edit`, which hands it here first
 * (`AutoWriter.rewrite`). A control move while a lane is being written
 * becomes the move plus the lane's new points, as one edit; the whole
 * pass is one undo step. When play stops, the written stretches are
 * thinned to the points that shape them.
 */

import type { CommandRunner } from "./media-pool";
import { DEFAULT_SPACE_POS, WRITE_LABEL, type AutomationMode, type BusDoc, type EnvelopeDoc, type EnvelopePointDoc, type Op, type SessionDoc, type SpaceParam, type TrackDoc } from "./session";
import type { SessionStore } from "./session";
import type { TransportState } from "./transport";
import { autoAt, newLane, targetKey, type AutoTarget } from "../panels/editor/automation";

/** A control let go: this long with no move of it ends a Touch pass. */
export const TOUCH_RELEASE_MS = 300;
/** How often held values are written on while playing. */
const TICK_MS = 50;

/** A control's new value, as an edit sets it. */
export interface Captured {
  owner: string;
  target: AutoTarget;
  value: number;
}

const SPACE_PARAMS: SpaceParam[] = ["x", "y", "z", "size", "width", "rotation", "lfe", "spread", "spatialize"];

function ownerOf(doc: SessionDoc, id: string): TrackDoc | BusDoc | null {
  return doc.tracks.find((t) => t.id === id) ?? (doc.buses ?? []).find((b) => b.id === id) ?? null;
}

/**
 * The lane values an edit sets, when it is a move of a track's or bus's
 * controls: its volume, pan, spot settings or effect settings. Anything
 * else (a rename, a new effect) captures nothing.
 */
export function capturedValues(doc: SessionDoc, op: Op): Captured[] {
  if (op.op === "set_track" || op.op === "set_bus") {
    const owner = ownerOf(doc, op.id);
    if (!owner) return [];
    const out: Captured[] = [];
    const c = op.changes;
    if (typeof c.volume_db === "number") out.push({ owner: op.id, target: { type: "volume" }, value: c.volume_db });
    if (typeof c.pan === "number") out.push({ owner: op.id, target: { type: "pan" }, value: c.pan });
    if (c.space) {
      const was = { ...DEFAULT_SPACE_POS, ...(owner.space ?? {}) };
      const now = { ...DEFAULT_SPACE_POS, ...c.space };
      for (const p of SPACE_PARAMS) if (now[p] !== was[p]) out.push({ owner: op.id, target: { type: "space", param: p }, value: now[p] ?? 0 });
    }
    return out;
  }
  if (op.op === "set_effects" && (op.target.kind === "track" || op.target.kind === "bus")) {
    const owner = ownerOf(doc, op.target.id);
    const was = owner?.effects?.slots ?? [];
    const now = op.chain.slots;
    // Only a settings move: the same effects in the same places.
    if (!owner || was.length !== now.length || was.some((s, i) => s.id !== now[i]!.id || s.effect !== now[i]!.effect)) return [];
    const out: Captured[] = [];
    for (let i = 0; i < now.length; i++) {
      const a = was[i]!.params;
      const b = now[i]!.params;
      for (const key of Object.keys(b)) {
        const v = b[key];
        if (typeof v === "number" && v !== a[key]) out.push({ owner: op.target.id, target: { type: "effect_param", slot_id: now[i]!.id, param: key }, value: v });
      }
    }
    return out;
  }
  return [];
}

/**
 * `points` with the stretch after `from` up to `to` replaced by one point
 * at `to` holding `value`. With `startValue`, a point at `from` holding it
 * begins the stretch, so the lane ramps from where it was rather than
 * from its next point.
 */
export function writeSpan(points: readonly EnvelopePointDoc[], from: number, to: number, value: number, startValue?: number): EnvelopePointDoc[] {
  const a = Math.max(0, Math.round(Math.min(from, to)));
  const b = Math.max(0, Math.round(Math.max(from, to)));
  const kept = points.filter((p) => p.position < a || p.position > b || (p.position === a && startValue === undefined && a !== b));
  const next = kept.filter((p) => p.position !== b);
  if (startValue !== undefined && a !== b && !next.some((p) => p.position === a)) next.push({ position: a, value: startValue, curve: "linear" });
  next.push({ position: b, value, curve: "linear" });
  next.sort((x, y) => x.position - y.position);
  return next;
}

/**
 * Drops the points between `from` and `to` that sit within `tolerance`
 * of the straight line through their neighbors, so a written pass keeps
 * only the points that shape it. Points outside the stretch stay.
 */
export function thinPoints(points: readonly EnvelopePointDoc[], from: number, to: number, tolerance: number): EnvelopePointDoc[] {
  const inside = points.filter((p) => p.position >= from && p.position <= to);
  if (inside.length < 3) return [...points];
  const keep = new Set<number>([0, inside.length - 1]);
  // Furthest-point splitting: keep the point that strays most from the line, then look at both halves.
  const stack: [number, number][] = [[0, inside.length - 1]];
  while (stack.length) {
    const [i0, i1] = stack.pop()!;
    const p0 = inside[i0]!;
    const p1 = inside[i1]!;
    let worst = -1;
    let dist = tolerance;
    for (let i = i0 + 1; i < i1; i++) {
      const p = inside[i]!;
      const f = p1.position === p0.position ? 0 : (p.position - p0.position) / (p1.position - p0.position);
      const d = Math.abs(p.value - (p0.value + (p1.value - p0.value) * f));
      if (d > dist) {
        dist = d;
        worst = i;
      }
    }
    if (worst >= 0) {
      keep.add(worst);
      stack.push([i0, worst], [worst, i1]);
    }
  }
  const thinned = inside.filter((_, i) => keep.has(i));
  return [...points.filter((p) => p.position < from || p.position > to), ...thinned].sort((x, y) => x.position - y.position);
}

/** One lane being written. */
interface Pass {
  target: AutoTarget;
  laneId: string;
  /** The lane's points as written so far (the store's own copy, never read back). */
  points: EnvelopePointDoc[];
  /** Where the lane was last written to. */
  last: number;
  /** Where this pass began. */
  from: number;
  value: number;
  /** When the control was last moved (Touch lets go after a pause). */
  moved: number;
  /** Value range, for thinning. */
  span: number;
}

export interface WriterDeps {
  session: Pick<SessionStore, "current" | "edit">;
  runner: () => CommandRunner | null;
  /** The value range of a lane, for thinning: the top and bottom it is drawn between. */
  rangeOf: (owner: TrackDoc | BusDoc, target: AutoTarget) => { top: number; bottom: number };
  /** Where a lane with no points sits: the control's own value. */
  restOf: (owner: TrackDoc | BusDoc, target: AutoTarget) => number;
  now?: () => number;
}

export class AutoWriter {
  private playing = false;
  private position = 0;
  private passes = new Map<string, Map<string, Pass>>();
  private timer: ReturnType<typeof setInterval> | null = null;
  private readonly now: () => number;
  private readonly listeners = new Set<(owners: string[]) => void>();
  private told = "";

  constructor(private readonly deps: WriterDeps) {
    this.now = deps.now ?? (() => Date.now());
  }

  /** True while any lane is being written. */
  get writing(): boolean {
    for (const m of this.passes.values()) if (m.size) return true;
    return false;
  }

  /** The owners whose lanes are being written right now. */
  get owners(): string[] {
    return [...this.passes.entries()].filter(([, m]) => m.size > 0).map(([id]) => id);
  }

  /** Hears which owners are being written, whenever that changes (headers light up). */
  subscribe(fn: (owners: string[]) => void): () => void {
    this.listeners.add(fn);
    fn(this.owners);
    return () => {
      this.listeners.delete(fn);
    };
  }

  private tell(): void {
    const owners = this.owners;
    const key = owners.join("|");
    if (key === this.told) return;
    this.told = key;
    for (const fn of this.listeners) fn(owners);
  }

  modeOf(doc: SessionDoc, owner: string): AutomationMode {
    return ownerOf(doc, owner)?.automation_mode ?? "read";
  }

  /** Follows the transport: writing runs while playing and ends when it stops. */
  onTransport(s: Pick<TransportState, "playing" | "position">): void {
    const was = this.playing;
    this.playing = s.playing;
    this.position = s.position;
    if (s.playing && !was) this.startWriteMode();
    if (!s.playing && was) this.finish();
  }

  /**
   * Sees an edit before it runs. A control move on an owner in Write,
   * Latch or Touch while playing comes back as the move plus the lane's
   * new points, labelled as a write; anything else comes back null.
   */
  rewrite(op: Op, _label: string): { op: Op; label: string } | null {
    const doc = this.deps.session.current;
    if (!this.playing || !doc) return null;
    const caught = capturedValues(doc, op).filter((c) => this.modeOf(doc, c.owner) !== "read");
    if (!caught.length) return null;
    const t = this.now();
    for (const c of caught) {
      const pass = this.passFor(doc, c.owner, c.target);
      pass.value = c.value;
      pass.moved = t;
      this.extend(pass, this.position);
    }
    this.ensureTicking();
    this.tell();
    const ops: Op[] = [op, ...this.laneOps(doc, caught.map((c) => c.owner))];
    return { op: { op: "batch", ops }, label: WRITE_LABEL };
  }

  private passFor(doc: SessionDoc, ownerId: string, target: AutoTarget): Pass {
    let mine = this.passes.get(ownerId);
    if (!mine) {
      mine = new Map();
      this.passes.set(ownerId, mine);
    }
    const key = targetKey(target);
    let pass = mine.get(key);
    if (pass) return pass;
    const owner = ownerOf(doc, ownerId)!;
    const lane = (owner.envelopes ?? []).find((e) => targetKey(e.target) === key) ?? newLane(target);
    const rest = this.deps.restOf(owner, target);
    const r = this.deps.rangeOf(owner, target);
    const at = this.position;
    pass = { target, laneId: lane.id, points: [...lane.points], last: at, from: at, value: autoAt(lane.points, at, rest), moved: this.now(), span: Math.abs(r.top - r.bottom) };
    // The pass starts from where the lane was, so the first move ramps from there.
    pass.points = writeSpan(pass.points, at, at, pass.value);
    mine.set(key, pass);
    return pass;
  }

  /** Writes the pass's value on from where it was last written to `to`. */
  private extend(pass: Pass, to: number): void {
    if (to < pass.last) {
      // Playback jumped back (a loop): the pass starts over from here.
      pass.from = to;
    }
    pass.points = writeSpan(pass.points, pass.last, to, pass.value);
    pass.last = to;
  }

  /** The set_envelopes edits that carry the written lanes of `owners`. */
  private laneOps(doc: SessionDoc, owners: readonly string[]): Op[] {
    const out: Op[] = [];
    for (const id of new Set(owners)) {
      const owner = ownerOf(doc, id);
      const mine = this.passes.get(id);
      if (!owner || !mine?.size) continue;
      let envelopes = [...(owner.envelopes ?? [])];
      for (const pass of mine.values()) {
        const i = envelopes.findIndex((e) => e.id === pass.laneId);
        const lane: EnvelopeDoc = i >= 0 ? { ...envelopes[i]!, enabled: true, points: pass.points } : { ...newLane(pass.target), id: pass.laneId, points: pass.points };
        if (i >= 0) envelopes[i] = lane;
        else envelopes = [...envelopes, lane];
      }
      out.push({ op: "set_envelopes", owner: id, envelopes });
    }
    return out;
  }

  /** Write mode: every lane of each owner in Write starts recording its control's value as play starts. */
  private startWriteMode(): void {
    const doc = this.deps.session.current;
    if (!doc) return;
    const all: (TrackDoc | BusDoc)[] = [...doc.tracks, ...(doc.buses ?? [])];
    let any = false;
    for (const o of all) {
      if (o.automation_mode !== "write") continue;
      for (const lane of o.envelopes ?? []) {
        const pass = this.passFor(doc, o.id, lane.target);
        pass.value = this.deps.restOf(o, lane.target);
        pass.points = writeSpan(pass.points, this.position, this.position, pass.value);
        any = true;
      }
    }
    if (any) {
      this.ensureTicking();
      this.emit(doc, this.owners);
    }
    this.tell();
  }

  private ensureTicking(): void {
    if (this.timer === null) this.timer = setInterval(() => this.tick(), TICK_MS);
  }

  /** Carries held values on to the playhead; lets Touch passes go after a pause. */
  private tick(): void {
    const doc = this.deps.session.current;
    if (!doc || !this.playing) return;
    const t = this.now();
    const changed: string[] = [];
    for (const [ownerId, mine] of this.passes) {
      const mode = this.modeOf(doc, ownerId);
      for (const [key, pass] of mine) {
        if (mode === "touch" && t - pass.moved > TOUCH_RELEASE_MS) {
          // Let go: the lane plays again from here. The pass's last point stands as written.
          this.thin(pass);
          const lanes = this.laneOps(doc, [ownerId]);
          mine.delete(key);
          // The thinned lane goes out now, before the pass is forgotten.
          const runner = this.deps.runner();
          if (runner && lanes.length) void this.deps.session.edit(WRITE_LABEL, lanes[0]!, runner).catch(console.error);
          continue;
        }
        if (mode === "read") {
          mine.delete(key);
          continue;
        }
        if (this.position !== pass.last) {
          this.extend(pass, this.position);
          changed.push(ownerId);
        }
      }
    }
    if (changed.length) this.emit(doc, changed);
    if (!this.writing && this.timer !== null) {
      clearInterval(this.timer);
      this.timer = null;
    }
    this.tell();
  }

  /** Keeps only the points that shape the written stretch; the caller sends the lane out. */
  private thin(pass: Pass): void {
    pass.points = thinPoints(pass.points, pass.from, pass.last, pass.span / 400);
  }

  private emit(doc: SessionDoc, owners: readonly string[]): void {
    const runner = this.deps.runner();
    const ops = this.laneOps(doc, owners);
    if (!runner || !ops.length) return;
    void this.deps.session.edit(WRITE_LABEL, ops.length === 1 ? ops[0]! : { op: "batch", ops }, runner).catch(console.error);
  }

  /** Play stopped: every pass ends, thinned, as the last edit of the step. */
  private finish(): void {
    const doc = this.deps.session.current;
    const owners = this.owners;
    if (doc) {
      for (const mine of this.passes.values()) for (const pass of mine.values()) this.thin(pass);
      if (owners.length) this.emit(doc, owners);
    }
    this.passes.clear();
    if (this.timer !== null) {
      clearInterval(this.timer);
      this.timer = null;
    }
    this.tell();
  }
}
