// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * Open libraries for the Libraries panel.
 *
 * Every change to a library (add, remove, rename, tag, collections) is
 * an undoable command that saves the whole index. Undo saves the index
 * as it was before. Copied files are never deleted by undo or remove;
 * Clean Up Library deletes files no item uses.
 *
 * Which libraries are open, and which one is showing, is a preference,
 * not an edit.
 */

import type { Library, LibraryCollection, LibraryImportResult, LibraryItem } from "../app/core";
import type { Command } from "../commands";
import type { CommandRunner } from "./media-pool";

export interface LibraryBackend {
  defaultRoot(): Promise<string>;
  create(dir: string, name: string): Promise<Library>;
  open(dir: string): Promise<Library>;
  save(dir: string, lib: Library): Promise<void>;
  import(dir: string, paths: string[], copy: boolean): Promise<LibraryImportResult>;
}

export interface OpenLibrary {
  readonly dir: string;
  readonly lib: Library;
}

export interface LibrariesState {
  readonly libs: readonly OpenLibrary[];
  readonly active: OpenLibrary | null;
  /** Collection id being shown, or null for all items. */
  readonly collection: string | null;
  readonly selected: ReadonlySet<string>;
}

/** Keeps open libraries and the shown one across restarts. */
export interface LibraryPrefs {
  get(): { open: string[]; active: string | null };
  set(open: string[], active: string | null): void;
}

type Listener = (s: LibrariesState) => void;

/** Path separator style of a folder path. */
function sepOf(dir: string): string {
  return dir.includes("\\") ? "\\" : "/";
}

export function isAbsolutePath(p: string): boolean {
  return p.startsWith("/") || p.startsWith("\\\\") || /^[A-Za-z]:[\\/]/.test(p);
}

export function joinPath(dir: string, rel: string): string {
  const sep = sepOf(dir);
  const cleanRel = rel.replace(/[\\/]/g, sep);
  return dir.endsWith(sep) ? dir + cleanRel : dir + sep + cleanRel;
}

/** Absolute path of an item's audio file. */
export function itemPath(dir: string, item: LibraryItem): string {
  return isAbsolutePath(item.file) ? item.file : joinPath(dir, item.file);
}

/** Folder-safe version of a library name. */
export function folderName(name: string): string {
  const cleaned = name
    .replace(/[<>:"/\\|?*]/g, " ")
    .replace(/\s+/g, " ")
    .trim()
    .replace(/[. ]+$/, "");
  return cleaned || "Library";
}

function samePath(a: string, b: string): boolean {
  return a.replace(/\\/g, "/").toLowerCase() === b.replace(/\\/g, "/").toLowerCase();
}

let counter = 0;
const newId = () =>
  typeof crypto !== "undefined" && "randomUUID" in crypto ? crypto.randomUUID() : `c-${++counter}`;

export class LibraryStore {
  private libs: OpenLibrary[] = [];
  private activeDir: string | null = null;
  private collection: string | null = null;
  private selection = new Set<string>();
  private restored = false;
  private readonly listeners = new Set<Listener>();
  private state: LibrariesState = { libs: [], active: null, collection: null, selected: new Set() };

  constructor(
    private readonly backend: LibraryBackend,
    private readonly prefs: LibraryPrefs,
  ) {}

  subscribe(fn: Listener): () => void {
    this.listeners.add(fn);
    fn(this.state);
    return () => {
      this.listeners.delete(fn);
    };
  }

  get current(): LibrariesState {
    return this.state;
  }

  /** Reopens the libraries that were open last time. Missing ones are skipped. */
  async restore(): Promise<string[]> {
    if (this.restored) return [];
    this.restored = true;
    const { open, active } = this.prefs.get();
    const failed: string[] = [];
    for (const dir of open) {
      if (this.libs.some((l) => samePath(l.dir, dir))) continue;
      try {
        const lib = await this.backend.open(dir);
        this.libs.push({ dir, lib });
      } catch {
        failed.push(dir);
      }
    }
    this.activeDir = this.libs.find((l) => active && samePath(l.dir, active))?.dir ?? this.libs[0]?.dir ?? null;
    this.emit(true);
    return failed;
  }

  /** Creates a library under the default folder, or in `dir` when given. */
  /**
   * Creates a library under the default folder, or in `dir` when given.
   * `bins` are created with it (the starter bins for a show library).
   */
  async create(name: string, dir?: string, bins: readonly { name: string; color: string | null }[] = []): Promise<OpenLibrary> {
    const target = dir ?? joinPath(await this.backend.defaultRoot(), folderName(name));
    const lib = await this.backend.create(target, name.trim() || "Library");
    if (bins.length > 0) {
      lib.collections.push(...bins.map((b) => ({ id: newId(), name: b.name, items: [], color: b.color })));
      await this.backend.save(target, lib);
    }
    return this.add({ dir: target, lib });
  }

  async openDir(dir: string): Promise<OpenLibrary> {
    const existing = this.libs.find((l) => samePath(l.dir, dir));
    if (existing) {
      this.show(existing.dir);
      return existing;
    }
    const lib = await this.backend.open(dir);
    return this.add({ dir, lib });
  }

  close(dir: string): void {
    this.libs = this.libs.filter((l) => !samePath(l.dir, dir));
    if (this.activeDir && samePath(this.activeDir, dir)) {
      this.activeDir = this.libs[0]?.dir ?? null;
      this.collection = null;
      this.selection.clear();
    }
    this.emit(true);
  }

  show(dir: string): void {
    if (this.activeDir === dir) return;
    this.activeDir = dir;
    this.collection = null;
    this.selection.clear();
    this.emit(true);
  }

  showCollection(id: string | null): void {
    this.collection = id;
    this.selection.clear();
    this.emit();
  }

  setSelection(ids: Iterable<string>): void {
    this.selection = new Set(ids);
    this.emit();
  }

  selectedItems(): LibraryItem[] {
    const lib = this.activeLib();
    return lib ? lib.lib.items.filter((i) => this.selection.has(i.id)) : [];
  }

  /** Items in the shown collection, in collection order, or all items. */
  shownItems(): LibraryItem[] {
    const lib = this.activeLib();
    if (!lib) return [];
    if (this.collection === null) return lib.lib.items;
    const c = lib.lib.collections.find((c) => c.id === this.collection);
    if (!c) return lib.lib.items;
    const byId = new Map(lib.lib.items.map((i) => [i.id, i]));
    return c.items.flatMap((id) => {
      const it = byId.get(id);
      return it ? [it] : [];
    });
  }

  // Edits. Each is one undo step.

  /** Copies files into the active library (or links them) and adds them. */
  async importPaths(paths: string[], runner: CommandRunner, copy = true): Promise<LibraryImportResult> {
    const target = this.activeLib();
    if (!target) throw new Error("Open or create a library first.");
    const result = await this.backend.import(target.dir, paths, copy);
    if (result.items.length > 0) {
      const ids = result.items.map((i) => i.id);
      const label = result.items.length === 1 ? "Add to Library" : `Add ${result.items.length} to Library`;
      await this.edit(
        label,
        (lib) => {
          lib.items.push(...result.items);
          const c = this.collection && lib.collections.find((c) => c.id === this.collection);
          if (c) c.items.push(...ids);
        },
        runner,
        target.dir,
      );
      this.setSelection(ids);
    }
    return result;
  }

  removeItems(ids: readonly string[], runner: CommandRunner): Promise<void> {
    const drop = new Set(ids);
    const n = ids.length;
    return this.edit(
      n === 1 ? "Remove from Library" : `Remove ${n} from Library`,
      (lib) => {
        lib.items = lib.items.filter((i) => !drop.has(i.id));
        for (const c of lib.collections) c.items = c.items.filter((id) => !drop.has(id));
      },
      runner,
    );
  }

  updateItem(id: string, changes: Partial<Pick<LibraryItem, "name" | "tags" | "notes" | "color">>, runner: CommandRunner): Promise<void> {
    const what = changes.name !== undefined ? "Rename" : changes.tags !== undefined ? "Edit Tags" : changes.notes !== undefined ? "Edit Notes" : "Set Color";
    return this.edit(
      `${what} Library Item`,
      (lib) => {
        const it = lib.items.find((i) => i.id === id);
        if (it) Object.assign(it, changes);
      },
      runner,
    );
  }

  async addCollection(name: string, runner: CommandRunner, withItems: readonly string[] = [], color: string | null = null): Promise<string> {
    const c: LibraryCollection = { id: newId(), name: name.trim() || "Bin", items: [...withItems], color };
    await this.edit("New Bin", (lib) => void lib.collections.push(c), runner);
    return c.id;
  }

  /** Adds several bins in one step, for example the starter bins. */
  addBins(bins: readonly { name: string; color: string | null }[], runner: CommandRunner): Promise<void> {
    if (bins.length === 0) return Promise.resolve();
    return this.edit(
      bins.length === 1 ? "New Bin" : `Add ${bins.length} Bins`,
      (lib) => void lib.collections.push(...bins.map((b) => ({ id: newId(), name: b.name, items: [], color: b.color }))),
      runner,
    );
  }

  setCollectionColor(id: string, color: string | null, runner: CommandRunner): Promise<void> {
    return this.edit(
      "Bin Color",
      (lib) => {
        const c = lib.collections.find((c) => c.id === id);
        if (c) c.color = color;
      },
      runner,
    );
  }

  /** Moves a bin to a new position in the bin list. */
  moveCollection(id: string, to: number, runner: CommandRunner): Promise<void> {
    return this.edit(
      "Move Bin",
      (lib) => {
        const from = lib.collections.findIndex((c) => c.id === id);
        if (from < 0) return;
        const [c] = lib.collections.splice(from, 1);
        lib.collections.splice(Math.max(0, Math.min(to, lib.collections.length)), 0, c!);
      },
      runner,
    );
  }

  setLibraryColor(color: string | null, runner: CommandRunner): Promise<void> {
    return this.edit("Show Color", (lib) => void (lib.color = color), runner);
  }

  renameCollection(id: string, name: string, runner: CommandRunner): Promise<void> {
    return this.edit(
      "Rename Bin",
      (lib) => {
        const c = lib.collections.find((c) => c.id === id);
        if (c) c.name = name.trim() || c.name;
      },
      runner,
    );
  }

  deleteCollection(id: string, runner: CommandRunner): Promise<void> {
    if (this.collection === id) this.collection = null;
    return this.edit("Delete Bin", (lib) => void (lib.collections = lib.collections.filter((c) => c.id !== id)), runner);
  }

  addToCollection(id: string, items: readonly string[], runner: CommandRunner): Promise<void> {
    return this.edit(
      "Add to Bin",
      (lib) => {
        const c = lib.collections.find((c) => c.id === id);
        if (!c) return;
        for (const i of items) if (!c.items.includes(i)) c.items.push(i);
      },
      runner,
    );
  }

  removeFromCollection(id: string, items: readonly string[], runner: CommandRunner): Promise<void> {
    const drop = new Set(items);
    return this.edit(
      "Remove from Bin",
      (lib) => {
        const c = lib.collections.find((c) => c.id === id);
        if (c) c.items = c.items.filter((i) => !drop.has(i));
      },
      runner,
    );
  }

  renameLibrary(name: string, runner: CommandRunner): Promise<void> {
    return this.edit("Rename Library", (lib) => void (lib.name = name.trim() || lib.name), runner);
  }

  /** Runs an edit on a copy of a library as one undoable command. */
  async edit(label: string, mutate: (lib: Library) => void, runner: CommandRunner, dir?: string): Promise<void> {
    const target = dir ? this.libs.find((l) => l.dir === dir) : this.activeLib();
    if (!target) throw new Error("Open or create a library first.");
    const before = target.lib;
    const after = structuredClone(before);
    mutate(after);
    await runner.execute(new LibraryEditCommand(this, target.dir, before, after, label));
  }

  // Used by LibraryEditCommand.

  async apply(dir: string, lib: Library): Promise<void> {
    await this.backend.save(dir, lib);
    const i = this.libs.findIndex((l) => l.dir === dir);
    if (i >= 0) this.libs[i] = { dir, lib };
    const ids = new Set(lib.items.map((it) => it.id));
    for (const id of [...this.selection]) if (!ids.has(id)) this.selection.delete(id);
    if (this.collection && !lib.collections.some((c) => c.id === this.collection)) this.collection = null;
    this.emit();
  }

  private activeLib(): OpenLibrary | null {
    return this.libs.find((l) => l.dir === this.activeDir) ?? null;
  }

  private add(open: OpenLibrary): OpenLibrary {
    this.libs.push(open);
    this.activeDir = open.dir;
    this.collection = null;
    this.selection.clear();
    this.emit(true);
    return open;
  }

  private emit(persist = false): void {
    this.state = {
      libs: [...this.libs],
      active: this.activeLib(),
      collection: this.collection,
      selected: new Set(this.selection),
    };
    if (persist) this.prefs.set(this.libs.map((l) => l.dir), this.activeDir);
    for (const fn of this.listeners) fn(this.state);
  }
}

class LibraryEditCommand implements Command {
  constructor(
    private readonly store: LibraryStore,
    private readonly dir: string,
    private readonly before: Library,
    private readonly after: Library,
    readonly label: string,
  ) {}

  do(): Promise<void> {
    return this.store.apply(this.dir, this.after);
  }

  undo(): Promise<void> {
    return this.store.apply(this.dir, this.before);
  }
}
