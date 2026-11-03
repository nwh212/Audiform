// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Waveform peaks for the multitrack view, loaded from the core's peak
 * cache on first use. A few files build at a time; each reports progress
 * so the clip can show how far along it is.
 */

import type { PeakSummary } from "../app/core";
import { pickLevel, type PeakLevel } from "../panels/editor/timeline";

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

/** Loaded peak levels, finest ones first to go, stay under this many bytes. */
export const PEAK_BUDGET = 96 * 1024 * 1024;

type Task = { path: string; level: number | null };

/**
 * Levels load as the view needs them: a file's coarsest level comes with
 * its summary (a few kilobytes), and finer ones only once a lane or the
 * Waveform editor is zoomed in far enough to use them. Fine levels of
 * long files are large (the finest level of an hour-long stereo file is
 * about 20 MB), so the least recently drawn ones are dropped when the
 * total goes past the budget; the coarsest level of each file stays.
 */
export class PeakStore {
  private readonly entries = new Map<string, PeakEntry>();
  private readonly listeners = new Set<() => void>();
  private readonly queue: Task[] = [];
  private readonly pending = new Set<string>();
  private readonly used = new Map<string, number>();
  private tick = 0;
  private bytes = 0;
  private running = 0;
  private unlisten: (() => void) | null = null;

  constructor(
    private readonly backend: PeakBackend,
    private readonly concurrency = Math.max(2, Math.min(4, Math.floor((globalThis.navigator?.hardwareConcurrency ?? 4) / 2))),
    private readonly budget = PEAK_BUDGET,
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
      this.enqueue({ path, level: null });
    }
    return e;
  }

  /**
   * The best loaded level for drawing at `framesPerPixel`, and a request
   * for the right one when it is not loaded yet (the store emits once it
   * is, so the view redraws). Until then the coarsest loaded level stands
   * in, a little blocky.
   */
  level(path: string, framesPerPixel: number): PeakLevel | null {
    const e = this.get(path);
    if (e.status !== "ready" || !e.summary) return null;
    const blocks = e.summary.levels.map((l) => l.block);
    // The coarsest level whose blocks fit in a pixel; the finest when none does.
    let want = 0;
    for (let i = 0; i < blocks.length; i++) if (blocks[i]! <= framesPerPixel && blocks[i]! >= (blocks[want] ?? 0)) want = i;
    if (!blocks.some((b) => b <= framesPerPixel)) want = blocks.indexOf(Math.min(...blocks));
    const have = e.levels.find((l) => l.block === blocks[want]);
    if (!have) this.enqueue({ path, level: want });
    const level = pickLevel(e.levels, framesPerPixel) ?? e.levels.reduce<PeakLevel | null>((a, l) => (!a || l.block > a.block ? l : a), null);
    if (level) this.used.set(`${path}#${level.block}`, ++this.tick);
    return level;
  }

  /** Forgets a file so the next `get` reloads it. */
  invalidate(path: string): void {
    const e = this.entries.get(path);
    if (e) for (const l of e.levels) this.drop(path, l);
    this.entries.delete(path);
    this.emit();
  }

  /** Bytes of peak data held right now. */
  get held(): number {
    return this.bytes;
  }

  private enqueue(t: Task): void {
    const key = `${t.path}#${t.level ?? "summary"}`;
    if (this.pending.has(key)) return;
    this.pending.add(key);
    this.queue.push(t);
    this.pump();
  }

  private pump(): void {
    while (this.running < this.concurrency && this.queue.length > 0) {
      const t = this.queue.shift()!;
      this.running++;
      void (t.level === null ? this.loadSummary(t.path) : this.loadLevel(t.path, t.level)).finally(() => {
        this.pending.delete(`${t.path}#${t.level ?? "summary"}`);
        this.running--;
        this.pump();
      });
    }
  }

  private async loadSummary(path: string): Promise<void> {
    const e = this.entries.get(path);
    if (!e) return;
    try {
      const summary = await this.backend.ensure(path);
      // The coarsest level comes with the summary: it is small and draws the whole file.
      let coarsest = 0;
      summary.levels.forEach((l, i) => {
        if (l.block > summary.levels[coarsest]!.block) coarsest = i;
      });
      const levels = summary.levels.length ? [await this.readLevel(path, coarsest, summary)] : [];
      Object.assign(e, { status: "ready", progress: 1, summary, levels, error: null });
      this.bytes += levels.reduce((n, l) => n + l.data.byteLength, 0);
    } catch (err) {
      Object.assign(e, { status: "error", error: err instanceof Error ? err.message : String(err) });
    }
    this.emit();
  }

  private async loadLevel(path: string, index: number): Promise<void> {
    const e = this.entries.get(path);
    if (!e || !e.summary || e.levels.some((l) => l.block === e.summary!.levels[index]?.block)) return;
    try {
      const level = await this.readLevel(path, index, e.summary);
      if (this.entries.get(path) !== e) return;
      e.levels.push(level);
      this.bytes += level.data.byteLength;
      this.used.set(`${path}#${level.block}`, ++this.tick);
      this.evict();
      this.emit();
    } catch {
      // The coarser level keeps drawing; the next zoom asks again.
    }
  }

  private async readLevel(path: string, index: number, summary: PeakSummary): Promise<PeakLevel> {
    const buf = await this.backend.level(path, index);
    // Some webviews hand raw bytes back as an array or a typed view, not an ArrayBuffer.
    const bytes = buf instanceof ArrayBuffer ? new Uint8Array(buf) : ArrayBuffer.isView(buf) ? new Uint8Array((buf as ArrayBufferView).buffer, (buf as ArrayBufferView).byteOffset, (buf as ArrayBufferView).byteLength) : Uint8Array.from(buf as unknown as ArrayLike<number>);
    const even = bytes.byteLength - (bytes.byteLength % 2);
    // A whole, even buffer is used in place; anything else is copied once.
    const data = bytes.byteOffset === 0 && even === bytes.buffer.byteLength ? new Int16Array(bytes.buffer) : new Int16Array(bytes.slice(0, even).buffer);
    return { block: summary.levels[index]!.block, data };
  }

  /** Drops the least recently drawn fine levels until the total fits the budget. */
  private evict(): void {
    while (this.bytes > this.budget) {
      let victim: { path: string; level: PeakLevel; at: number } | null = null;
      for (const [path, e] of this.entries) {
        if (e.levels.length < 2) continue;
        const coarsest = e.levels.reduce((a, l) => (l.block > a.block ? l : a));
        for (const l of e.levels) {
          if (l === coarsest) continue;
          const at = this.used.get(`${path}#${l.block}`) ?? 0;
          if (!victim || at < victim.at) victim = { path, level: l, at };
        }
      }
      if (!victim) return;
      this.drop(victim.path, victim.level);
    }
  }

  private drop(path: string, level: PeakLevel): void {
    const e = this.entries.get(path);
    if (e) e.levels = e.levels.filter((l) => l !== level);
    this.bytes -= level.data.byteLength;
    this.used.delete(`${path}#${level.block}`);
  }

  private emit(): void {
    for (const fn of this.listeners) fn();
  }
}
