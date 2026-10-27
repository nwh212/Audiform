// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Play, pause and stop, plus the live playhead and level meters.
 *
 * While playing, the core is asked for its position and peak levels
 * about 30 times a second. The playhead in the session view is only
 * written when playback stops, so playing never marks the session as
 * changed.
 */

import type { TransportStatus } from "../app/core";
import { sessionEnd, type SessionStore } from "./session";

export interface TransportBackend {
  play(from: number): Promise<void>;
  stop(): Promise<number | null>;
  status(): Promise<TransportStatus>;
  /** True when playback should stop by itself at the end of the audio (not while looping). */
  stopAtEnd?: () => boolean;
  /** True when playback jumps over the time selection (Skip Selection), so play never moves into it. */
  skipsSelection?: () => boolean;
}

/**
 * Where Play starts: the playhead, or the start of the time selection
 * when the playhead is outside it (unless playback skips the selection).
 */
export function playStart(playhead: number, selection: { start: number; end: number } | null | undefined, skips: boolean): number {
  if (!selection || skips || selection.end <= selection.start) return playhead;
  return playhead >= selection.start && playhead < selection.end ? playhead : selection.start;
}

/** Meter level with a slow fall and a held peak, in 0..1 (above 1 means over). */
export interface MeterLevel {
  level: number;
  peak: number;
  /** True once any sample went over full scale, until reset. */
  over: boolean;
}

export interface TransportState {
  playing: boolean;
  /** Paused: stopped by Pause, which picks up again where it left off. */
  paused: boolean;
  /**
   * The start marker: where playback began (session samples). It stays put
   * through Pause, and moves to the playhead on Stop (then it is null).
   */
  anchor: number | null;
  /** Session samples. */
  position: number;
  master: [MeterLevel, MeterLevel];
  /** Levels after the fader by track id, and by bus id while the mix is routed. */
  tracks: Record<string, [MeterLevel, MeterLevel]>;
  /** Audio FX meters by track or bus id, and `master` for the master chain: left in, right in, left out, right out. Only chains that do something. */
  chains: Record<string, [MeterLevel, MeterLevel, MeterLevel, MeterLevel]>;
  /** 3D Space sessions: the master's speaker channels in the layout's order, after its fader. Missing otherwise. */
  bed?: MeterLevel[];
}

/** How lively the level meters are: Settings > Playback and Recording. */
export type MeterSpeed = "fast" | "medium" | "slow";
const BALLISTICS: Record<MeterSpeed, { fall: number; rise: number; hold: number }> = {
  // Fall per tick (about 30 a second), share of a rise taken per tick, peak hold in ticks.
  fast: { fall: 0.86, rise: 1, hold: 30 },
  medium: { fall: 0.93, rise: 0.6, hold: 45 },
  slow: { fall: 0.965, rise: 0.3, hold: 60 },
};
let ballistics = BALLISTICS.fast;
export function setMeterSpeed(speed: MeterSpeed): void {
  ballistics = BALLISTICS[speed] ?? BALLISTICS.fast;
}

export const quiet = (): MeterLevel => ({ level: 0, peak: 0, over: false });

/** One meter reading folded into the last: slow fall, held peak. `hold` counts ticks left. */
export function foldMeter(prev: MeterLevel, reading: number, hold: number): { meter: MeterLevel; hold: number } {
  const b = ballistics;
  const risen = reading > prev.level ? prev.level + (reading - prev.level) * b.rise : reading;
  const level = Math.max(risen, prev.level * b.fall);
  let peak = prev.peak;
  // The peak marker always shows the true peak, whatever the speed.
  if (reading >= peak) {
    peak = reading;
    hold = b.hold;
  } else if (hold > 0) {
    hold--;
  } else {
    peak = Math.max(level, peak * b.fall);
  }
  return { meter: { level, peak, over: prev.over || reading > 1 }, hold };
}

/** Key of the master chain's meters in `TransportState.chains`. */
export const MASTER_CHAIN = "master";

type Listener = (s: TransportState) => void;

export class TransportStore {
  private state: TransportState = { playing: false, paused: false, anchor: null, position: 0, master: [quiet(), quiet()], tracks: {}, chains: {} };
  private readonly listeners = new Set<Listener>();
  private timer: ReturnType<typeof setTimeout> | null = null;
  private holds = new Map<string, number>();
  private busy: Promise<unknown> = Promise.resolve();
  /** Set while recording, so playback never stops at the end of the old audio. */
  holdAtEnd = false;
  /** Return to Start on Stop: Stop puts the playhead back on the start marker. */
  returnOnStop = false;

  constructor(
    private readonly backend: TransportBackend,
    /** The document this transport plays. */
    readonly session: SessionStore,
    private readonly onError: (e: unknown) => void = console.error,
    private readonly tickMs = 33,
  ) {}

  subscribe(fn: Listener): () => void {
    this.listeners.add(fn);
    fn(this.state);
    return () => {
      this.listeners.delete(fn);
    };
  }

  get current(): TransportState {
    return this.state;
  }

  /**
   * Plays from the playhead, or from `from` (session samples). The start
   * marker goes where playback starts, except when resuming from Pause.
   */
  play(from?: number): Promise<void> {
    return this.serial(async () => {
      const v = this.session.current?.view;
      const at = Math.max(0, Math.round(from ?? playStart(v?.playhead ?? 0, v?.selection, this.backend.skipsSelection?.() ?? false)));
      await this.session.flushView?.();
      await this.backend.play(at);
      const resume = this.state.paused && from === undefined && this.state.anchor !== null;
      this.set({ playing: true, paused: false, position: at, anchor: resume ? this.state.anchor : at });
      this.loop();
    });
  }

  /**
   * Pause on and off: stops with the playhead where it reached and the
   * start marker where it was; pressed again, plays on from the playhead.
   */
  pause(): Promise<void> {
    if (this.state.paused) return this.play();
    if (!this.state.playing) return Promise.resolve();
    return this.serial(async () => {
      const pos = await this.backend.stop();
      this.halt(pos ?? this.state.position, true);
    });
  }

  /**
   * Stops with the playhead where playback reached, and brings the start
   * marker to it. With Return to Start on Stop, the playhead goes back to
   * the start marker instead.
   */
  stop(): Promise<void> {
    return this.serial(async () => {
      const was = this.state.playing;
      const pos = was ? await this.backend.stop() : null;
      const here = was ? (pos ?? this.state.position) : (this.session.current?.view.playhead ?? 0);
      this.halt(this.returnOnStop && this.state.anchor !== null ? this.state.anchor : here);
    });
  }

  /** Space: plays (or plays on from Pause), and stops while playing. */
  toggle(): Promise<void> {
    return this.state.playing ? this.stop() : this.play();
  }

  /** Moves playback to a new spot while playing, or the playhead while stopped. */
  seek(to: number): Promise<void> {
    if (this.state.playing) return this.play(to);
    const v = this.session.current?.view;
    if (v) this.session.setView({ ...v, playhead: Math.max(0, Math.round(to)) });
    return Promise.resolve();
  }

  /** Stops the clock. `paused` keeps the start marker; otherwise it moves to the playhead. */
  private halt(position: number, paused = false): void {
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
    const v = this.session.current?.view;
    if (v) this.session.setView({ ...v, playhead: Math.max(0, Math.round(position)) });
    // Levels drop at once; over lights stay lit until clicked.
    const rest = (m: MeterLevel): MeterLevel => ({ level: 0, peak: 0, over: m.over });
    const tracks: Record<string, [MeterLevel, MeterLevel]> = {};
    for (const [id, [l, r]] of Object.entries(this.state.tracks)) if (l.over || r.over) tracks[id] = [rest(l), rest(r)];
    this.set({ playing: false, paused, anchor: paused ? this.state.anchor : null, position, master: [rest(this.state.master[0]), rest(this.state.master[1])], tracks, chains: {} });
  }

  /** Clears the over lights of one track, or of the master with no id. */
  clearOver(trackId?: string): void {
    const off = (m: MeterLevel): MeterLevel => ({ ...m, over: false });
    if (trackId === undefined) {
      this.set({ master: [off(this.state.master[0]), off(this.state.master[1])], ...(this.state.bed ? { bed: this.state.bed.map(off) } : {}) });
      return;
    }
    const t = this.state.tracks[trackId];
    if (t) this.set({ tracks: { ...this.state.tracks, [trackId]: [off(t[0]), off(t[1])] } });
  }

  private loop(): void {
    if (this.timer) clearTimeout(this.timer);
    const tick = async () => {
      if (!this.state.playing) return;
      try {
        const s = await this.backend.status();
        if (!this.state.playing) return;
        if (!s.playing) {
          this.halt(this.state.position);
          return;
        }
        // Stop at the end of the last clip instead of playing on into silence.
        const doc = this.session.current;
        const end = doc ? sessionEnd(doc) : 0;
        if (end > 0 && !this.holdAtEnd && this.backend.stopAtEnd?.() && s.position >= end && (this.state.anchor ?? 0) < end) {
          await this.backend.stop();
          this.halt(end);
          return;
        }
        this.applyStatus(s);
      } catch (e) {
        this.onError(e);
      }
      if (this.state.playing) this.timer = setTimeout(() => void tick(), this.tickMs);
    };
    this.timer = setTimeout(() => void tick(), this.tickMs);
  }

  /** Folds a status reading into the meters. Public for tests. */
  applyStatus(s: TransportStatus): void {
    const tracks: Record<string, [MeterLevel, MeterLevel]> = {};
    s.trackIds.forEach((id, i) => {
      const m = s.meters[i] ?? [0, 0];
      const prev = this.state.tracks[id] ?? [quiet(), quiet()];
      tracks[id] = [this.meter(`${id}:0`, prev[0], m[0]), this.meter(`${id}:1`, prev[1], m[1])];
    });
    const chains: TransportState["chains"] = {};
    (s.busIds ?? []).forEach((id, i) => {
      const m = s.busMeters?.[i] ?? [0, 0];
      const prev = this.state.tracks[id] ?? [quiet(), quiet()];
      tracks[id] = [this.meter(`${id}:0`, prev[0], m[0]), this.meter(`${id}:1`, prev[1], m[1])];
    });
    const chain = (id: string, m: [number, number, number, number] | null | undefined) => {
      if (!m) return;
      const prev = this.state.chains[id] ?? [quiet(), quiet(), quiet(), quiet()];
      chains[id] = [0, 1, 2, 3].map((k) => this.meter(`${id}:fx${k}`, prev[k]!, m[k]!)) as TransportState["chains"][string];
    };
    s.trackIds.forEach((id, i) => chain(id, s.chainMeters?.[i]));
    (s.busIds ?? []).forEach((id, i) => chain(id, s.busChainMeters?.[i]));
    chain(MASTER_CHAIN, s.masterChainMeter);
    this.set({
      position: s.position,
      tracks,
      chains,
      master: [this.meter("m:0", this.state.master[0], s.master[0]), this.meter("m:1", this.state.master[1], s.master[1])],
      // A status without speaker readings keeps the rows (they fall as with silence) instead of blanking them.
      bed: s.bedMeters?.length
        ? s.bedMeters.map((v, k) => this.meter(`bed:${k}`, this.state.bed?.[k] ?? quiet(), v))
        : (this.state.bed ?? []).map((m, k) => this.meter(`bed:${k}`, m, 0)),
    });
  }

  private meter(key: string, prev: MeterLevel, reading: number): MeterLevel {
    const r = foldMeter(prev, reading, this.holds.get(key) ?? 0);
    this.holds.set(key, r.hold);
    return r.meter;
  }

  private set(patch: Partial<TransportState>): void {
    this.state = { ...this.state, ...patch };
    for (const fn of this.listeners) fn(this.state);
  }

  /** Runs transport calls one at a time so fast key presses stay in order. */
  private serial(fn: () => Promise<void>): Promise<void> {
    const run = this.busy.then(fn, fn);
    this.busy = run.catch((e: unknown) => this.onError(e));
    return run;
  }
}

/** Meter position for a 0..1 peak on a -60 dB to 0 dB scale. */
export function meterFraction(peak: number, floorDb = -60): number {
  if (peak <= 0) return 0;
  const db = 20 * Math.log10(peak);
  return Math.max(0, Math.min(1, (db - floorDb) / -floorDb));
}
