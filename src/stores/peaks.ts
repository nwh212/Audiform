// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Waveform peaks for the multitrack view, loaded from the core's peak
 * cache on first use. A few files build at a time; each reports progress
 * so the clip can show how far along it is.
 */

import type { PeakSummary } from "../app/core";
import type { PeakLevel } from "../panels/editor/timeline";

export interface PeakEntry {
  status: "building" | "ready" | "error";
  progress: number;
  summary: PeakSummary | null;
  levels: PeakLevel[];
  error: string | null;
}

export interface PeakBackend {
  ensure(path: string): Promise<PeakSummary>;
  level(path: string, level: number): Promise<ArrayBuffer>;
  /** Subscribes to build progress. Returns an unsubscribe function. */
  onProgress(fn: (path: string, progress: number) => void): Promise<() => void>;
}

export class PeakStore {
  private readonly entries = new Map<string, PeakEntry>();
  private readonly listeners = new Set<() => void>();
  private readonly queue: string[] = [];
  private running = 0;
  private unlisten: (() => void) | null = null;

  constructor(
    private readonly backend: PeakBackend,
    private readonly concurrency = Math.max(2, Math.min(4, Math.floor((globalThis.navigator?.hardwareConcurrency ?? 4) / 2))),
  ) {}

  /** Calls `fn` whenever any entry changes. */
  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    if (!this.unlisten) {
      void this.backend
        .onProgress((path, progress) => {
          const e = this.entries.get(path);
          if (e && e.status === "building") {
            e.progress = progress;
            this.emit();
          }
        })
        .then((off) => (this.unlisten = off))
        .catch(() => {});
    }
    return () => {
      this.listeners.delete(fn);
    };
  }

  /** The entry for a file, starting a load if this is the first request. */
  get(path: string): PeakEntry {
    let e = this.entries.get(path);
    if (!e) {
      e = { status: "building", progress: 0, summary: null, levels: [], error: null };
      this.entries.set(path, e);
      this.queue.push(path);
      this.pump();
    }
    return e;
  }

  /** Forgets a file so the next `get` reloads it. */
  invalidate(path: string): void {
    this.entries.delete(path);
    this.emit();
  }

  private pump(): void {
    while (this.running < this.concurrency && this.queue.length > 0) {
      const path = this.queue.shift()!;
      this.running++;
      void this.load(path).finally(() => {
        this.running--;
        this.pump();
      });
    }
  }

  private async load(path: string): Promise<void> {
    const e = this.entries.get(path);
    if (!e) return;
    try {
      const summary = await this.backend.ensure(path);
      const levels: PeakLevel[] = [];
      for (let i = 0; i < summary.levels.length; i++) {
        const buf = await this.backend.level(path, i);
        // Some webviews hand raw bytes back as an array or a typed view, not an ArrayBuffer.
        const bytes = buf instanceof ArrayBuffer ? new Uint8Array(buf) : ArrayBuffer.isView(buf) ? new Uint8Array((buf as ArrayBufferView).buffer, (buf as ArrayBufferView).byteOffset, (buf as ArrayBufferView).byteLength) : Uint8Array.from(buf as unknown as ArrayLike<number>);
        const even = bytes.byteLength - (bytes.byteLength % 2);
        levels.push({ block: summary.levels[i]!.block, data: new Int16Array(bytes.slice(0, even).buffer) });
      }
      Object.assign(e, { status: "ready", progress: 1, summary, levels, error: null });
    } catch (err) {
      Object.assign(e, { status: "error", error: err instanceof Error ? err.message : String(err) });
    }
    this.emit();
  }

  private emit(): void {
    for (const fn of this.listeners) fn();
  }
}
