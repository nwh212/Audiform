// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Recording state for the transport bars. App owns the logic and sets
 * `toggle`; the bars only show the state and call it.
 */

import { writable } from "svelte/store";
import { LIVE_BLOCK } from "../app/core";
import type { PeakLevel } from "../panels/editor/timeline";

export interface RecordingState {
  active: boolean;
  /** Seconds recorded so far. */
  seconds: number;
  /** Input peak, 0 to 1. */
  peak: number;
}

export const recording = writable<RecordingState>({ active: false, seconds: 0, peak: 0 });

/** Input level per track id (0 to 1, falling smoothly), while inputs are open. */
export const inputLevels = writable<Record<string, number>>({});

/**
 * The waveform of one take while it records, kept in the same form as a
 * finished file's peak cache (16-bit min and max per block and channel,
 * 32 frames per block, coarser past 10 and 60 minutes exactly as the
 * cache chooses), so it is drawn by the same code and nothing changes
 * when the take is saved.
 */
export class LiveWave {
  channels = 1;
  /** Frames per block now: 32, then 64 past 10 minutes, then 256 past an hour. */
  block = LIVE_BLOCK;
  blocks = 0;
  /** Finest blocks received so far. */
  private fine = 0;
  private buf = new Int16Array(1 << 16);

  /** Min and max per block, laid out like a peak level. */
  level(): PeakLevel {
    return { block: this.block, data: this.buf.subarray(0, this.blocks * this.channels * 2) };
  }

  private cache = new Map<number, { blocks: number; data: Int16Array }>();

  /**
   * The level a finished file's peaks would use at `framesPerPixel`: the
   * largest of 32 (or 64, 256), 256, 4,096 and 65,536 frames per block
   * that is no larger than it, merged from the finest data.
   */
  levelFor(framesPerPixel: number): PeakLevel {
    const sizes = [this.block, ...[256, 4096, 65536].filter((b) => b > this.block)];
    let pick = sizes[0]!;
    for (const b of sizes) if (b <= framesPerPixel) pick = b;
    if (pick === this.block) return this.level();
    const hit = this.cache.get(pick);
    if (hit && hit.blocks === this.blocks) return { block: pick, data: hit.data };
    const factor = pick / this.block;
    const stride = this.channels * 2;
    const out = Math.ceil(this.blocks / factor);
    const data = new Int16Array(out * stride);
    for (let o = 0; o < out; o++) {
      for (let k = 0; k < this.channels; k++) {
        let mn = 32767;
        let mx = -32768;
        for (let i = o * factor; i < Math.min(this.blocks, (o + 1) * factor); i++) {
          mn = Math.min(mn, this.buf[i * stride + k * 2]!);
          mx = Math.max(mx, this.buf[i * stride + k * 2 + 1]!);
        }
        data[o * stride + k * 2] = mn;
        data[o * stride + k * 2 + 1] = mx;
      }
    }
    this.cache.set(pick, { blocks: this.blocks, data });
    return { block: pick, data };
  }

  /** Frames received so far. */
  get frames(): number {
    return this.fine * LIVE_BLOCK;
  }

  push(channels: number, flat: number[], rate: number) {
    const ch = Math.max(1, Math.min(2, channels));
    if (this.fine === 0) this.channels = ch;
    const stride = this.channels * 2;
    const factor = this.block / LIVE_BLOCK;
    const incoming = Math.floor(flat.length / stride);
    for (let j = 0; j < incoming; j++) {
      const idx = Math.floor((this.fine + j) / factor);
      const at = idx * stride;
      if (at + stride > this.buf.length) {
        const bigger = new Int16Array(this.buf.length * 2);
        bigger.set(this.buf);
        this.buf = bigger;
      }
      const fresh = idx >= this.blocks;
      for (let k = 0; k < this.channels; k++) {
        const mn = flat[j * stride + k * 2]!;
        const mx = flat[j * stride + k * 2 + 1]!;
        if (fresh) {
          this.buf[at + k * 2] = mn;
          this.buf[at + k * 2 + 1] = mx;
        } else {
          if (mn < this.buf[at + k * 2]!) this.buf[at + k * 2] = mn;
          if (mx > this.buf[at + k * 2 + 1]!) this.buf[at + k * 2 + 1] = mx;
        }
      }
      if (fresh) this.blocks = idx + 1;
    }
    this.fine += incoming;
    const seconds = this.frames / Math.max(1, rate);
    if (this.block === LIVE_BLOCK && seconds > 600) this.coarsen(2);
    else if (this.block === LIVE_BLOCK * 2 && seconds > 3600) this.coarsen(4);
  }

  /** Merges every `factor` blocks into one (exact min and max). */
  private coarsen(factor: number) {
    const stride = this.channels * 2;
    const out = Math.ceil(this.blocks / factor);
    for (let o = 0; o < out; o++) {
      for (let k = 0; k < this.channels; k++) {
        let mn = 32767;
        let mx = -32768;
        for (let i = o * factor; i < Math.min(this.blocks, (o + 1) * factor); i++) {
          mn = Math.min(mn, this.buf[i * stride + k * 2]!);
          mx = Math.max(mx, this.buf[i * stride + k * 2 + 1]!);
        }
        this.buf[o * stride + k * 2] = mn;
        this.buf[o * stride + k * 2 + 1] = mx;
      }
    }
    this.blocks = out;
    this.block *= factor;
  }
}

/** The waveform of takes in progress, drawn on their tracks while recording. */
export interface LiveTakes {
  /** Session sample where the takes start. */
  at: number;
  /** Device sample rate. */
  rate: number;
  /** Per track id (or "wave" for the Waveform editor). */
  waves: Record<string, LiveWave>;
  /** Changes whenever any wave grows, so drawings refresh. */
  tick: number;
}
export const liveTakes = writable<LiveTakes | null>(null);

/** Starts or stops recording. Set by App. */
export const recordControl: { toggle: (() => void) | null } = { toggle: null };
