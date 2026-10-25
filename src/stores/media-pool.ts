// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Altered Audio Editor.

/**
 * The session's audio files, as listed in the Files panel.
 *
 * Adding and removing files are undoable commands. Measuring a file
 * (probing) is not an edit, so its results land on the item whether or
 * not the item is currently in the list. That way Undo then Redo of an
 * import brings back the finished item, not a half-probed one.
 */

import type { AudioInfo } from "../app/core";
import type { Command } from "../commands";

export type MediaStatus = "probing" | "ready" | "error";

export interface MediaItem {
  readonly id: string;
  readonly path: string;
  /** File name with extension. */
  readonly name: string;
  readonly status: MediaStatus;
  readonly info: AudioInfo | null;
  /** User-facing reason when status is "error". */
  readonly error: string | null;
}

export interface MediaPoolState {
  /** Files in import order. */
  readonly items: readonly MediaItem[];
  readonly selected: ReadonlySet<string>;
}

export interface MediaPoolDeps {
  probe(path: string): Promise<AudioInfo>;
  expand(paths: string[]): Promise<string[]>;
  /** Files measured at the same time. Default 4. */
  concurrency?: number;
  /** Treat paths that differ only in case as the same file. Default true (Windows). */
  ignorePathCase?: boolean;
  newId?: () => string;
}

/** Anything that can run a command: an UndoStack in the app, a fake in tests. */
export interface CommandRunner {
  execute(command: Command): Promise<void>;
}

export interface ImportResult {
  added: number;
  /** Already in the list. */
  duplicates: number;
  /** Every listed item for the given paths, new or existing, in path order. */
  ids: string[];
}

type Listener = (state: MediaPoolState) => void;

/** File name from a Windows or POSIX path. */
export function fileName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

/**
 * Comparable form of a path. Where the OS ignores case (Windows, macOS)
 * both slash styles mean the same thing too. On Linux a path is compared
 * exactly, since case and backslashes are part of the name there.
 */
function pathKey(path: string, ignoreCase: boolean): string {
  return ignoreCase ? path.replace(/\\/g, "/").toLowerCase() : path;
}

function plural(n: number, one: string, many: string): string {
  return n === 1 ? one : `${n} ${many}`;
}

let counter = 0;
const defaultId = () =>
  typeof crypto !== "undefined" && "randomUUID" in crypto ? crypto.randomUUID() : `media-${++counter}`;

export class MediaPool {
  private readonly known = new Map<string, MediaItem>();
  private order: string[] = [];
  private selection = new Set<string>();
  private readonly listeners = new Set<Listener>();
  private state: MediaPoolState = { items: [], selected: new Set() };

  private readonly waiting: string[] = [];
  private running = 0;
  private idle: Promise<void> = Promise.resolve();
  private settle: (() => void) | null = null;

  private readonly concurrency: number;
  private readonly newId: () => string;
  private readonly ignoreCase: boolean;

  /** Where imported files should be used from (a copy in the session folder, say). */
  private placer: ((paths: string[]) => Promise<string[]>) | null = null;

  constructor(private readonly deps: MediaPoolDeps) {
    this.concurrency = Math.max(1, deps.concurrency ?? 4);
    this.newId = deps.newId ?? defaultId;
    this.ignoreCase = deps.ignorePathCase ?? true;
  }

  /** Svelte store contract. */
  subscribe(fn: Listener): () => void {
    this.listeners.add(fn);
    fn(this.state);
    return () => {
      this.listeners.delete(fn);
    };
  }

  get current(): MediaPoolState {
    return this.state;
  }

  get(id: string): MediaItem | undefined {
    return this.order.includes(id) ? this.known.get(id) : undefined;
  }

  /** Selected items in list order. */
  selectedItems(): MediaItem[] {
    return this.state.items.filter((it) => this.selection.has(it.id));
  }

  /**
   * Sets the step that decides where newly imported files are used from:
   * it gets the files not listed yet and returns a path for each, for
   * example a copy in the session folder. Null uses them where they are.
   */
  setPlacer(fn: ((paths: string[]) => Promise<string[]>) | null): void {
    this.placer = fn;
  }

  /**
   * Adds files and folders as one undoable step. Folders are searched
   * for audio. Files already in the list are skipped. New files are
   * selected and measured in the background.
   */
  async importPaths(paths: readonly string[], runner: CommandRunner): Promise<ImportResult> {
    if (paths.length === 0) return { added: 0, duplicates: 0, ids: [] };
    let expanded = await this.deps.expand([...paths]);
    const present = new Map(this.order.map((id) => [pathKey(this.known.get(id)!.path, this.ignoreCase), id]));
    if (this.placer) {
      const fresh = [...new Set(expanded.filter((p) => !present.has(pathKey(p, this.ignoreCase))))];
      if (fresh.length) {
        const placed = await this.placer(fresh);
        const to = new Map(fresh.map((p, i) => [p, placed[i] ?? p]));
        expanded = expanded.map((p) => to.get(p) ?? p);
      }
    }
    const fresh: MediaItem[] = [];
    const all: string[] = [];
    let duplicates = 0;
    for (const path of expanded) {
      const key = pathKey(path, this.ignoreCase);
      const existing = present.get(key);
      if (existing) {
        duplicates++;
        if (!all.includes(existing)) all.push(existing);
        continue;
      }
      const item: MediaItem = { id: this.newId(), path, name: fileName(path), status: "probing", info: null, error: null };
      present.set(key, item.id);
      fresh.push(item);
      all.push(item.id);
    }
    if (fresh.length === 0) return { added: 0, duplicates, ids: all };

    for (const item of fresh) this.known.set(item.id, item);
    const ids = fresh.map((it) => it.id);
    await runner.execute(new InsertMediaCommand(this, ids, `Import ${plural(ids.length, "File", "Files")}`));
    this.setSelection(ids);
    for (const id of ids) this.waiting.push(id);
    this.pump();
    return { added: ids.length, duplicates, ids: all };
  }

  /** Removes items as one undoable step. */
  /** Removing files as a command not run yet, to combine with others into one undo step. */
  removeCommand(ids: readonly string[]): Command | null {
    const present = ids.filter((id) => this.order.includes(id));
    return present.length ? new RemoveMediaCommand(this, present, `Remove ${plural(present.length, "File", "Files")}`) : null;
  }

  async remove(ids: readonly string[], runner: CommandRunner): Promise<void> {
    const present = ids.filter((id) => this.order.includes(id));
    if (present.length === 0) return;
    await runner.execute(new RemoveMediaCommand(this, present, `Remove ${plural(present.length, "File", "Files")}`));
  }

  /**
   * Replaces the whole list, as when a session is opened or a new one
   * started. Not undoable: it resets history along with the session.
   * Every file is measured again in the background.
   */
  replaceAll(entries: readonly { id: string; path: string }[]): void {
    this.known.clear();
    this.waiting.length = 0;
    this.order = [];
    this.selection.clear();
    for (const { id, path } of entries) {
      this.known.set(id, { id, path, name: fileName(path), status: "probing", info: null, error: null });
      this.order.push(id);
      this.waiting.push(id);
    }
    this.emit();
    this.pump();
  }

  /**
   * Adds a session's files to the list and keeps the files already open,
   * as when a session is opened or a new one started, so switching
   * sessions never loses imported files. A file already listed takes the
   * session's id, since its clips refer to it by that. Not undoable: it
   * resets history along with the session.
   */
  merge(entries: readonly { id: string; path: string }[]): void {
    const ignore = this.deps.ignorePathCase ?? true;
    const byPath = new Map<string, string>();
    for (const id of this.order) {
      const it = this.known.get(id);
      if (it) byPath.set(pathKey(it.path, ignore), id);
    }
    this.selection.clear();
    for (const { id, path } of entries) {
      const old = byPath.get(pathKey(path, ignore));
      if (old === id) continue;
      if (old !== undefined) {
        const it = this.known.get(old)!;
        this.known.delete(old);
        this.known.set(id, { ...it, id });
        this.order = this.order.map((o) => (o === old ? id : o));
        const w = this.waiting.indexOf(old);
        if (w >= 0) this.waiting[w] = id;
        // Being measured under the old id: measure again under the new one.
        else if (it.status === "probing") this.waiting.push(id);
        byPath.set(pathKey(path, ignore), id);
        continue;
      }
      if (this.known.has(id)) {
        if (!this.order.includes(id)) this.order.push(id);
        continue;
      }
      this.known.set(id, { id, path, name: fileName(path), status: "probing", info: null, error: null });
      this.order.push(id);
      this.waiting.push(id);
      byPath.set(pathKey(path, ignore), id);
    }
    this.emit();
    this.pump();
  }

  /**
   * Points files at new paths (their copies in the session folder) as a
   * command not run yet, to combine with the session's own change into
   * one undo step. Null when nothing changes.
   */
  repathCommand(changes: readonly { id: string; path: string }[], label: string): Command | null {
    const list = changes.filter((c) => this.known.has(c.id) && this.known.get(c.id)!.path !== c.path);
    if (!list.length) return null;
    const back = list.map((c) => ({ id: c.id, path: this.known.get(c.id)!.path }));
    return {
      label,
      do: () => this.repath(list),
      undo: () => this.repath(back),
    };
  }

  private repath(changes: readonly { id: string; path: string }[]): void {
    for (const { id, path } of changes) {
      const it = this.known.get(id);
      if (!it) continue;
      this.known.set(id, { ...it, path, name: fileName(path), info: it.info ? { ...it.info, path, name: fileName(path) } : null });
    }
    this.emit();
  }

  /** Ready files, for saving with the session. */
  readyItems(): MediaItem[] {
    return this.state.items.filter((it) => it.status === "ready" && it.info);
  }

  /** Measures a file again, for example after it was reconnected. */
  retry(id: string): void {
    const item = this.known.get(id);
    if (!item || item.status === "probing") return;
    this.update({ ...item, status: "probing", error: null });
    this.waiting.push(id);
    this.pump();
  }

  setSelection(ids: Iterable<string>): void {
    this.selection = new Set([...ids].filter((id) => this.order.includes(id)));
    this.emit();
  }

  selectAll(): void {
    this.setSelection(this.order);
  }

  /** Resolves once every queued file has been measured. For tests and shutdown. */
  whenIdle(): Promise<void> {
    return this.idle;
  }

  // Used by the commands below. Not for components.

  /** Puts ids back at their recorded positions, lowest index first. */
  insert(entries: readonly { id: string; index: number }[]): void {
    const sorted = [...entries].sort((a, b) => a.index - b.index);
    for (const { id, index } of sorted) {
      if (this.order.includes(id)) continue;
      this.order.splice(Math.min(index, this.order.length), 0, id);
    }
    this.emit();
  }

  /** Takes ids out and reports where each one was. */
  detach(ids: readonly string[]): { id: string; index: number }[] {
    const drop = new Set(ids);
    const removed = this.order.flatMap((id, index) => (drop.has(id) ? [{ id, index }] : []));
    this.order = this.order.filter((id) => !drop.has(id));
    for (const id of ids) this.selection.delete(id);
    this.emit();
    return removed;
  }

  get size(): number {
    return this.order.length;
  }

  private update(item: MediaItem): void {
    this.known.set(item.id, item);
    this.emit();
  }

  private pump(): void {
    if (this.running === 0 && this.waiting.length > 0 && !this.settle) {
      this.idle = new Promise((resolve) => (this.settle = resolve));
    }
    while (this.running < this.concurrency && this.waiting.length > 0) {
      const id = this.waiting.shift()!;
      const item = this.known.get(id);
      if (!item) continue;
      this.running++;
      this.deps
        .probe(item.path)
        .then(
          (info) => ({ ...this.known.get(id)!, status: "ready" as const, info, error: null }),
          (e: unknown) => ({
            ...this.known.get(id)!,
            status: "error" as const,
            info: null,
            error: e instanceof Error ? e.message : String(e),
          }),
        )
        .then((next) => {
          this.running--;
          // Skipped when the file was given another id meanwhile (see merge).
          if (this.known.get(id)?.id === id) this.update(next);
          this.pump();
        });
    }
    if (this.running === 0 && this.waiting.length === 0 && this.settle) {
      this.settle();
      this.settle = null;
    }
  }

  private emit(): void {
    this.state = {
      items: this.order.map((id) => this.known.get(id)!),
      selected: new Set(this.selection),
    };
    for (const fn of this.listeners) fn(this.state);
  }
}

class InsertMediaCommand implements Command {
  private positions: { id: string; index: number }[];

  constructor(
    private readonly pool: MediaPool,
    ids: readonly string[],
    readonly label: string,
  ) {
    const start = pool.size;
    this.positions = ids.map((id, i) => ({ id, index: start + i }));
  }

  do(): void {
    this.pool.insert(this.positions);
  }

  undo(): void {
    this.positions = this.pool.detach(this.positions.map((p) => p.id));
  }
}

class RemoveMediaCommand implements Command {
  private positions: { id: string; index: number }[] = [];

  constructor(
    private readonly pool: MediaPool,
    private readonly ids: readonly string[],
    readonly label: string,
  ) {}

  do(): void {
    this.positions = this.pool.detach(this.ids);
  }

  undo(): void {
    this.pool.insert(this.positions);
    this.pool.setSelection(this.ids);
  }
}
