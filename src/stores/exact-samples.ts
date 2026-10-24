// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

/**
 * Exact samples of files, read on demand for drawing when zoomed in past
 * what the peak cache holds (the Multitrack lanes and the Waveform
 * editor). Each file keeps one stretch around what was last asked for.
 * Subscribers are told when new samples arrive, so views redraw.
 */

import { mediaFrames } from "../app/core";

export interface Frames {
  /** First frame held. */
  start: number;
  /** Frames held. */
  count: number;
  channels: number;
  /** Interleaved samples. */
  data: Float32Array;
}

/** Zoomed in past this many source frames per pixel, views draw exact samples. */
export const SAMPLE_VIEW = 8;

class ExactSamples {
  private held = new Map<string, Frames>();
  private asking = new Set<string>();
  private failed = new Set<string>();
  private listeners = new Set<() => void>();

  /** The samples held for `path` if they cover frames [from, to); otherwise asks for them and returns null. */
  get(path: string, from: number, to: number): Frames | null {
    const have = this.held.get(path);
    if (have && from >= have.start && to <= have.start + have.count + 1) return have;
    this.request(path, from, to);
    return null;
  }

  private request(path: string, from: number, to: number): void {
    const span = Math.max(1, to - from);
    const start = Math.max(0, Math.floor(from - span));
    const count = Math.ceil(span * 3) + 2;
    const key = `${path}:${start}:${count}`;
    if (this.asking.has(key)) return;
    this.asking.add(key);
    void mediaFrames(path, start, count)
      .then((r) => {
        this.held.set(path, { start, count: r.data.length / r.channels, channels: r.channels, data: r.data });
        for (const fn of this.listeners) fn();
      })
      .catch((e: unknown) => {
        // The peak drawing stays up; say why once per file.
        if (!this.failed.has(path)) console.warn(`Exact samples could not be read from ${path}:`, e);
        this.failed.add(path);
      })
      .finally(() => this.asking.delete(key));
  }

  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }
}

export const exactSamples = new ExactSamples();
