// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Output level while a file previews from Files, Drive Manager or
 * Libraries, so the Levels panel moves for previews too. Polls the core
 * quickly while a preview plays and slowly while idle.
 */

import { foldMeter, type MeterLevel } from "./transport";

export interface PreviewMeterState {
  playing: boolean;
  master: [MeterLevel, MeterLevel];
}

export interface PreviewMeterBackend {
  status(): Promise<{ playing: boolean; peak?: [number, number] }>;
}

const quiet = (): MeterLevel => ({ level: 0, peak: 0, over: false });

export class PreviewMeter {
  private state: PreviewMeterState = { playing: false, master: [quiet(), quiet()] };
  private readonly listeners = new Set<(s: PreviewMeterState) => void>();
  private holds: [number, number] = [0, 0];
  private timer: ReturnType<typeof setTimeout> | null = null;

  constructor(
    private readonly backend: PreviewMeterBackend,
    private readonly fastMs = 40,
    private readonly idleMs = 300,
  ) {}

  subscribe(fn: (s: PreviewMeterState) => void): () => void {
    this.listeners.add(fn);
    fn(this.state);
    if (this.listeners.size === 1) this.schedule(0);
    return () => {
      this.listeners.delete(fn);
      if (this.listeners.size === 0 && this.timer) {
        clearTimeout(this.timer);
        this.timer = null;
      }
    };
  }

  clearOver(): void {
    this.set({ master: [{ ...this.state.master[0], over: false }, { ...this.state.master[1], over: false }] });
  }

  /** Folds one status reading in. Public for tests. */
  apply(s: { playing: boolean; peak?: [number, number] }): void {
    if (!s.playing) {
      if (this.state.playing) {
        const rest = (m: MeterLevel): MeterLevel => ({ level: 0, peak: 0, over: m.over });
        this.set({ playing: false, master: [rest(this.state.master[0]), rest(this.state.master[1])] });
      }
      return;
    }
    const p = s.peak ?? [0, 0];
    const l = foldMeter(this.state.master[0], p[0], this.holds[0]);
    const r = foldMeter(this.state.master[1], p[1], this.holds[1]);
    this.holds = [l.hold, r.hold];
    this.set({ playing: true, master: [l.meter, r.meter] });
  }

  private schedule(ms: number): void {
    if (this.timer) clearTimeout(this.timer);
    this.timer = setTimeout(() => void this.tick(), ms);
  }

  private async tick(): Promise<void> {
    if (this.listeners.size === 0) return;
    try {
      this.apply(await this.backend.status());
    } catch {
      // Status can fail while a preview is starting or stopping; try again next tick.
    }
    if (this.listeners.size > 0) this.schedule(this.state.playing ? this.fastMs : this.idleMs);
  }

  private set(patch: Partial<PreviewMeterState>): void {
    this.state = { ...this.state, ...patch };
    for (const fn of this.listeners) fn(this.state);
  }
}
